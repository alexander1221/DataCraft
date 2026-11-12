package datacraft

import java.nio.charset.StandardCharsets
import java.nio.file.{Files,Paths}
import java.util.UUID
import org.apache.hadoop.fs.Path
import org.apache.spark.sql.{Row,SparkSession}
import org.apache.spark.sql.types._
import ujson.{Value,Str}

/** Opt-in acceptance against real HDFS, never an HTTP mock or local filesystem alias. */
class HdfsIntegrationSuite extends munit.FunSuite {
  private val enabled=sys.env.get("DATACRAFT_HDFS_TESTS").contains("1")
  private var spark:SparkSession=_
  override def beforeAll():Unit=if(enabled) {
    spark=SparkSession.builder().master("local[2]").appName("DataCraft real HDFS acceptance")
      .config("spark.ui.enabled","false").config("spark.driver.host","127.0.0.1")
      .config("spark.driver.bindAddress","127.0.0.1").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }
  override def afterAll():Unit=if(spark!=null) spark.stop()
  private def config(writing:Boolean,path:String):Value={
    val name=if(writing) "hdfs_export_text" else "hdfs_import_text"
    val project=ujson.read(Files.readString(Paths.get(s"../../examples/pipelines/$name.dcraft")))
    val c=project("nodes").arr.find(_("config")("kind").str==name).get("config")
    c("hdfs_path")=Str(path);c("null_value")=Str("NA")
    c("host")=Str(sys.env.getOrElse("DATACRAFT_HDFS_HOST","localhost"))
    c("port")=ujson.Num(sys.env.getOrElse("DATACRAFT_HDFS_PORT","9870").toInt)
    c
  }
  private def withRoot(test: String=>Unit):Unit={
    assume(enabled,"NOT_RUN: set DATACRAFT_HDFS_TESTS=1 for real HDFS acceptance")
    val root="/datacraft-tests/"+UUID.randomUUID().toString
    HdfsClient.withClient(config(true,root+"/file.tsv")) {fs=>require(!fs.exists(new Path(root)),"Test namespace collision")}
    try test(root) finally HdfsClient.withClient(config(true,root+"/file.tsv")) {fs=>
      require(root.startsWith("/datacraft-tests/") && root.split('/').length==3,"Invalid test cleanup namespace")
      fs.delete(new Path(root),true)
    }
  }
  private def frame(text:String):org.apache.spark.sql.DataFrame={
    import scala.jdk.CollectionConverters._
    spark.createDataFrame(Seq(Row(1L,text,Double.box(1.25)),Row(2L,"🙂",null)).asJava,
      new StructType().add("id",LongType,false).add("name",StringType,true).add("amount",DoubleType,true))
  }
  private def bytes(path:String):Array[Byte]=HdfsClient.withClient(config(false,path)) {fs=>
    val input=fs.open(new Path(path));try input.readAllBytes() finally input.close()
  }
  test("real WebHDFS CREATE/OPEN redirects, single-file UTF8 export, explicit column casts and nullable round trip") {withRoot {root=>
    val target=root+"/данные.tsv"
    HdfsText.write(frame("строка\t\"цитата\"\r\nline\rend"),config(true,target))
    HdfsClient.withClient(config(false,target)) {fs=>assert(fs.getFileStatus(new Path(target)).isFile)}
    assert(new String(bytes(target),StandardCharsets.UTF_8).startsWith("id\tname\tamount\n"))
    val read=config(false,target);read("infer_schema")=ujson.Bool(false)
    read("column_options")=Str("[{\"column\":1,\"type\":\"long\"},{\"column\":3,\"type\":\"double\"}]")
    val imported=HdfsText.read(spark,read)
    try {
      val rows=imported.table.orderBy("id").collect().toSeq
      assertEquals(rows,Seq(Row(1L,"строка\t\"цитата\"\r\nline\rend",1.25),Row(2L,"🙂",null)))
      assertEquals(imported.table.schema.fields.map(_.dataType).toSeq,Seq(LongType,StringType,DoubleType))
    } finally Files.deleteIfExists(imported.spool)
  }}
  test("real HDFS import handles explicit ANSI charset, decimal comma, skip/limit, dates, missing values and omitted columns") {withRoot {root=>
    val target=root+"/ansi.txt"
    val payload="ignored\r\nsum;date;omit\r\nруб 12,50;12.11.2026;x\r\n\r\nNA;13.11.2026;y\r\n"
    HdfsClient.withClient(config(true,target)) {fs=>fs.mkdirs(new Path(root));val out=fs.create(new Path(target),false);try out.write(payload.getBytes("windows-1251")) finally out.close()}
    val read=config(false,target); read("delimiter")=Str(";"); read("record_separator")=Str("\\r\\n")
    read("encoding")=Str("windows-1251");read("skip_rows")=ujson.Num(1);read("decimal_separator")=Str(",")
    read("column_options")=Str("[{\"column\":1,\"name\":\"amount\",\"type\":\"double\",\"remove_non_numeric\":true,\"missing_value\":\"-999\"},{\"column\":2,\"type\":\"date\",\"input_format\":\"dd.MM.uuuu\"},{\"column\":3,\"omit\":true}]")
    val imported=HdfsText.read(spark,read)
    try assertEquals(imported.table.collect().toSeq,Seq(Row(12.5,java.sql.Date.valueOf("2026-11-12")),Row(-999.0,java.sql.Date.valueOf("2026-11-13")))) finally Files.deleteIfExists(imported.spool)
    read("max_rows")=ujson.Num(1)
    val limited=HdfsText.read(spark,read);try assertEquals(limited.table.count(),1L) finally Files.deleteIfExists(limited.spool)
  }}
  test("existing destination survives rejected overwrite and failed encoding; temporary uploads are cleaned") {withRoot {root=>
    val target=root+"/safe.tsv";HdfsText.write(frame("original"),config(true,target));val original=bytes(target).toSeq
    intercept[IllegalArgumentException](HdfsText.write(frame("replacement"),config(true,target)))
    assertEquals(bytes(target).toSeq,original)
    val failing=config(true,target);failing("overwrite")=ujson.Bool(true);failing("encoding")=Str("US-ASCII")
    intercept[java.nio.charset.UnmappableCharacterException](HdfsText.write(frame("Я"),failing))
    assertEquals(bytes(target).toSeq,original)
    HdfsClient.withClient(config(false,target)) {fs=>assert(!fs.listStatus(new Path(root)).exists(_.getPath.getName.endsWith(".tmp")))}
    val replacement=config(true,target);replacement("overwrite")=ujson.Bool(true)
    HdfsText.write(frame("changed"),replacement)
    assert(new String(bytes(target),StandardCharsets.UTF_8).contains("changed"))
  }}
  test("directory browsing, anonymous/Simple reads and missing file errors use the real server") {withRoot {root=>
    val target=root+"/list.tsv";HdfsText.write(frame("listed"),config(true,target))
    val settings=config(false,target)
    assertEquals(HdfsClient.list(settings,root)("rows")(0)("path").str,target)
    settings("authentication")=Str("none")
    assert(HdfsClient.list(settings,root)("rows").arr.nonEmpty)
    intercept[java.io.FileNotFoundException](HdfsText.read(spark,config(false,root+"/absent")))
  }}
  test("Simple user permissions, refused connection and malformed records report real failures") {withRoot {root=>
    val target=root+"/owner.tsv";HdfsText.write(frame("owner"),config(true,target))
    val denied=config(true,root+"/private/new.tsv");denied("username")=Str("datacraft_no_write_user")
    intercept[java.io.IOException](HdfsText.write(frame("forbidden"),denied))
    val refused=config(false,target);refused("port")=ujson.Num(1);refused("timeout_seconds")=ujson.Num(1)
    intercept[java.io.IOException](HdfsClient.list(refused,root))
    val malformed=root+"/malformed.tsv"
    HdfsClient.withClient(config(true,malformed)) {fs=>
      val out=fs.create(new Path(malformed),false);try out.write("a\tb\n1\t2\t3\n".getBytes(StandardCharsets.UTF_8)) finally out.close()
    }
    intercept[IllegalArgumentException](HdfsText.read(spark,config(false,malformed)))
  }}
}
