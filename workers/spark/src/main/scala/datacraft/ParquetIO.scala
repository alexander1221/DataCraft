package datacraft

import java.nio.file.{Files,Path,StandardCopyOption}
import org.apache.hadoop.conf.Configuration
import org.apache.parquet.hadoop.{ParquetFileWriter,ParquetWriter}
import org.apache.parquet.hadoop.api.WriteSupport
import org.apache.parquet.hadoop.metadata.CompressionCodecName
import org.apache.parquet.io.{LocalOutputFile,OutputFile}
import org.apache.spark.sql.{DataFrame,SparkSession}
import org.apache.spark.sql.catalyst.InternalRow
import org.apache.spark.sql.execution.datasources.parquet.ParquetWriteSupport
import ujson.Value
import scala.util.Using

/** Single-file binary Parquet I/O. Version-bound Spark adapter: docs/PARQUET_IO.md. */
object ParquetIO {
  private class Builder(output: OutputFile) extends ParquetWriter.Builder[InternalRow,Builder](output) {
    override protected def self(): Builder = this
    override protected def getWriteSupport(conf: Configuration): WriteSupport[InternalRow] = new ParquetWriteSupport
  }

  private def localPath(value:String):Path={
    require(!value.contains("://"),"Parquet destination must be a local filesystem path")
    val path=java.nio.file.Paths.get(value)
    require(path.isAbsolute && path.getFileName!=null,"Parquet destination requires a resolved absolute file path")
    path
  }
  private def writeFile(df:DataFrame,path:Path,config:Value):Unit=df.sparkSession.withActive {
    require(df.schema.nonEmpty,"Cannot write a Parquet file without columns")
    val codec=config("compression").str match {
      case "uncompressed"=>CompressionCodecName.UNCOMPRESSED
      case "snappy"=>CompressionCodecName.SNAPPY
      case "gzip"=>CompressionCodecName.GZIP
      case "zstd"=>CompressionCodecName.ZSTD
      case _=>throw new IllegalArgumentException("Unsupported Parquet compression")
    }
    val group=config("row_group_mb").num.toInt
    require(group>=1 && group<=1024,"Parquet row group must be between 1 and 1024 MiB")
    val conf=new Configuration(df.sparkSession.sparkContext.hadoopConfiguration)
    ParquetWriteSupport.setSchema(df.schema,conf)
    conf.setBoolean("spark.sql.parquet.writeLegacyFormat",false)
    conf.set("spark.sql.parquet.outputTimestampType","TIMESTAMP_MICROS")
    conf.setBoolean("spark.sql.parquet.fieldId.write.enabled",true)
    val builder=new Builder(new LocalOutputFile(path))
    Using.resource(builder.withConf(conf).withWriteMode(ParquetFileWriter.Mode.OVERWRITE)
      .withCompressionCodec(codec).withRowGroupSize(group.toLong*1024*1024).withValidation(true).build()) {writer=>
      // Copy reused UnsafeRows before the iterator materializes a partition on the driver.
      val rows=df.queryExecution.toRdd.map(_.copy()).toLocalIterator
      while(rows.hasNext) writer.write(rows.next())
    }
  }

  def writeLocal(df:DataFrame,config:Value):Unit={
    val target=localPath(config("path").str)
    val parent=target.getParent
    val overwrite=config("overwrite").bool
    require(!Files.exists(target) || (overwrite && Files.isRegularFile(target)),"Destination exists or is not a file; enable overwrite explicitly")
    if(!Files.exists(parent)) {require(config("create_parents").bool,"Parent directory does not exist");Files.createDirectories(parent)}
    require(Files.isDirectory(parent),"Parquet destination parent is not a directory")
    val temporary=Files.createTempFile(parent,".datacraft-parquet-",".tmp")
    try {
      writeFile(df,temporary,config)
      if(overwrite) Files.move(temporary,target,StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING)
      else Files.move(temporary,target)
    } finally Files.deleteIfExists(temporary)
  }

  def readHdfs(spark:SparkSession,config:Value):(DataFrame,Path)=HdfsClient.withClient(config) {fs=>
    val source=HdfsClient.path(config("hdfs_path").str)
    require(fs.getFileStatus(source).isFile,"HDFS Parquet import currently requires a single file")
    val spool=Files.createTempFile("datacraft-hdfs-parquet-",".parquet")
    var success=false
    try {
      Using.resource(fs.open(source))(input=>Files.copy(input,spool,StandardCopyOption.REPLACE_EXISTING))
      val df=spark.read.parquet(spool.toString)
      success=true;(df,spool)
    } finally {if(!success) Files.deleteIfExists(spool)}
  }

  def writeHdfs(df:DataFrame,config:Value):Unit=HdfsClient.withClient(config) {fs=>
    val target=HdfsClient.path(config("hdfs_path").str)
    val spool=Files.createTempFile("datacraft-parquet-export-",".parquet")
    try {
      HdfsClient.publish(fs,target,config("overwrite").bool,config("create_parents").bool) {temporary=>
        writeFile(df,spool,config)
        Using.resource(fs.create(temporary,false))(output=>Files.copy(spool,output))
      }
    } finally Files.deleteIfExists(spool)
  }
}
