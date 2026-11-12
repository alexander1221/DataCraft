package datacraft

import java.nio.file.Files
import java.util.UUID
import org.apache.hadoop.fs.Path
import org.apache.spark.sql.{Row,SparkSession}
import org.apache.spark.sql.types._
import scala.jdk.CollectionConverters._
import ujson.{Obj,Value}

/** Real NameNode/DataNode acceptance, opt-in like HdfsIntegrationSuite. */
class ParquetHdfsSuite extends munit.FunSuite {
  private val enabled=sys.env.get("DATACRAFT_HDFS_TESTS").contains("1")
  private var spark:SparkSession=_
  override def beforeAll():Unit=if(enabled) {
    spark=SparkSession.builder().master("local[2]").appName("DataCraft HDFS Parquet acceptance")
      .config("spark.ui.enabled","false").config("spark.driver.host","127.0.0.1")
      .config("spark.driver.bindAddress","127.0.0.1").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }
  override def afterAll():Unit=if(spark!=null) spark.stop()
  private def config(path:String):Value=Obj("hdfs_path"->path,"host"->ujson.Str(sys.env.getOrElse("DATACRAFT_HDFS_HOST","localhost")),
    "port"->sys.env.getOrElse("DATACRAFT_HDFS_PORT","9870").toInt,"protocol"->"webhdfs","authentication"->"simple",
    "username"->"hadoop","timeout_seconds"->15,"overwrite"->false,"create_parents"->true,
    "compression"->"snappy","row_group_mb"->1)
  private def withRoot(test:String=>Unit):Unit={
    assume(enabled,"NOT_RUN: DATACRAFT_HDFS_TESTS=1 requires real HDFS")
    val root="/datacraft-tests/"+UUID.randomUUID().toString
    try test(root) finally HdfsClient.withClient(config(root+"/file")) {fs=>
      require(root.startsWith("/datacraft-tests/") && root.split('/').length==3)
      fs.delete(new Path(root),true)
    }
  }
  private def table=spark.createDataFrame(Seq(Row(9007199254740993L,"данные 🙂",new java.math.BigDecimal("12345678901234567890.1234567890")),
    Row(2L,null,null)).asJava,new StructType().add("id",LongType,false).add("text",StringType,true).add("amount",DecimalType(38,10),true))

  test("real HDFS single-file binary Parquet preserves types, exact values and overwrite protections") {withRoot {root=>
    val settings=config(root+"/данные.parquet")
    ParquetIO.writeHdfs(table,settings)
    HdfsClient.withClient(settings) {fs=>
      assert(fs.getFileStatus(new Path(settings("hdfs_path").str)).isFile)
      val in=fs.open(new Path(settings("hdfs_path").str))
      try assertEquals(new String(in.readNBytes(4),java.nio.charset.StandardCharsets.US_ASCII),"PAR1") finally in.close()
    }
    intercept[IllegalArgumentException](ParquetIO.writeHdfs(table,settings))
    val (df,spool)=ParquetIO.readHdfs(spark,settings)
    try {
      val rows=df.orderBy("id").collect()
      assertEquals(rows(1).getLong(0),9007199254740993L)
      assertEquals(rows(1).getString(1),"данные 🙂")
      assertEquals(rows(1).getDecimal(2).toPlainString,"12345678901234567890.1234567890")
      assert(rows(0).isNullAt(1));assertEquals(df.schema("amount").dataType,DecimalType(38,10))
    } finally Files.deleteIfExists(spool)
    settings("overwrite")=ujson.Bool(true)
    ParquetIO.writeHdfs(table.limit(1),settings)
    val (replacement,temporary)=ParquetIO.readHdfs(spark,settings)
    try assertEquals(replacement.count(),1L) finally Files.deleteIfExists(temporary)
    HdfsClient.withClient(settings)(fs=>assertEquals(fs.listStatus(new Path(root)).length,1))
  }}
  test("failed HDFS serialization preserves the previous target and malformed or missing Parquet is recoverable") {withRoot {root=>
    val settings=config(root+"/safe.parquet")
    ParquetIO.writeHdfs(table,settings);settings("overwrite")=ujson.Bool(true)
    intercept[Exception](ParquetIO.writeHdfs(spark.range(1).selectExpr("cast(null as void) as unsupported"),settings))
    val (df,spool)=ParquetIO.readHdfs(spark,settings)
    try assertEquals(df.count(),2L) finally Files.deleteIfExists(spool)
    intercept[java.io.FileNotFoundException](ParquetIO.readHdfs(spark,config(root+"/missing.parquet")))
    val bad=config(root+"/bad.parquet")
    HdfsClient.withClient(bad) {fs=>val out=fs.create(new Path(bad("hdfs_path").str),false);try out.writeUTF("not parquet") finally out.close()}
    intercept[org.apache.spark.SparkException](ParquetIO.readHdfs(spark,bad))
  }}
}
