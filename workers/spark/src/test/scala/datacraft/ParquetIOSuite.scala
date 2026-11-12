package datacraft

import java.nio.file.{Files,Path}
import java.nio.charset.StandardCharsets
import org.apache.parquet.hadoop.ParquetFileReader
import org.apache.parquet.io.LocalInputFile
import org.apache.parquet.schema.LogicalTypeAnnotation
import org.apache.spark.ml.attribute.{AttributeGroup,NumericAttribute}
import org.apache.spark.ml.linalg.{SQLDataTypes,Vectors,Vector}
import org.apache.spark.sql.{Row,SparkSession}
import org.apache.spark.sql.types._
import ujson.{Obj,Str,Value}
import scala.jdk.CollectionConverters._

class ParquetIOSuite extends munit.FunSuite {
  private var spark:SparkSession=_
  private val large=9007199254740993L
  override def beforeAll():Unit={
    spark=SparkSession.builder().master("local[2]").appName("DataCraft single-file Parquet tests")
      .config("spark.ui.enabled","false").config("spark.driver.host","127.0.0.1")
      .config("spark.driver.bindAddress","127.0.0.1").config("spark.sql.session.timeZone","UTC").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }
  override def afterAll():Unit=if(spark!=null) spark.stop()
  private def config(path:Path,codec:String="snappy"):Value=Obj("path"->path.toString,"overwrite"->false,
    "create_parents"->false,"compression"->codec,"row_group_mb"->1)
  private def table()={
    val vectorMetadata=new AttributeGroup("features",Array[org.apache.spark.ml.attribute.Attribute](NumericAttribute.defaultAttr.withName("signal"))).toMetadata()
    val schema=new StructType().add("id",LongType,false,new MetadataBuilder().putLong("large_id",large).build())
      .add("name",StringType,true).add("amount",DecimalType(38,10),true).add("day",DateType,true)
      .add("at",TimestampType,true).add("items",ArrayType(IntegerType,true),true)
      .add("labels",MapType(StringType,LongType,true),true).add("features",SQLDataTypes.VectorType,true,vectorMetadata)
    spark.createDataFrame(Seq(
      Row(large,"строка\n🙂",new java.math.BigDecimal("12345678901234567890.1234567890"),java.sql.Date.valueOf("2026-11-12"),
        java.sql.Timestamp.valueOf("2026-11-12 12:34:56.123456"),Seq(1,null,3),Map("x"->7L),Vectors.dense(2.5)),
      Row(2L,"",null,null,null,Seq.empty[Int],Map.empty[String,Long],Vectors.sparse(1,Array.empty[Int],Array.empty[Double]))
    ).asJava,schema)
  }
  private def clean(path:Path):Unit=Files.deleteIfExists(path)

  test("real Parquet footer, schema and exact nested/ML/decimal/date/timestamp values round trip for every advertised codec") {
    val directory=Files.createTempDirectory("datacraft-parquet-test-")
    try {
      for(codec<-Seq("uncompressed","snappy","gzip","zstd")) {
        val path=directory.resolve(codec+".parquet")
        try {
          ParquetIO.writeLocal(table(),config(path,codec))
          assert(Files.isRegularFile(path))
          val input=Files.newInputStream(path);try assertEquals(new String(input.readNBytes(4),StandardCharsets.US_ASCII),"PAR1") finally input.close()
          val footer=ParquetFileReader.open(new LocalInputFile(path))
          try {
            assertEquals(footer.getFooter.getBlocks.asScala.map(_.getRowCount).sum,2L)
            assert(footer.getFooter.getBlocks.asScala.flatMap(_.getColumns.asScala).forall(_.getCodec.name().equalsIgnoreCase(codec)))
            val decimal=footer.getFooter.getFileMetaData.getSchema.getFields.asScala.find(_.getName=="amount").get.asPrimitiveType().getLogicalTypeAnnotation.asInstanceOf[LogicalTypeAnnotation.DecimalLogicalTypeAnnotation]
            assertEquals(decimal.getPrecision,38);assertEquals(decimal.getScale,10)
          } finally footer.close()
          val result=spark.read.parquet(path.toString)
          val rows=result.orderBy("id").collect()
          assertEquals(rows(1).getLong(0),large);assertEquals(rows(1).getString(1),"строка\n🙂")
          assertEquals(rows(1).getDecimal(2).toPlainString,"12345678901234567890.1234567890")
          assertEquals(rows(1).getDate(3),java.sql.Date.valueOf("2026-11-12"))
          assertEquals(rows(1).getTimestamp(4),java.sql.Timestamp.valueOf("2026-11-12 12:34:56.123456"))
          assertEquals(rows(1).getSeq[Integer](5).toSeq,Seq(Integer.valueOf(1),null,Integer.valueOf(3)))
          assertEquals(rows(1).getMap[String,Long](6).toMap,Map("x"->7L))
          assertEquals(rows(1).getAs[Vector](7),Vectors.dense(2.5))
          assert(rows(0).isNullAt(2));assertEquals(rows(0).getString(1),"")
          assertEquals(result.schema("id").metadata.getLong("large_id"),large)
          assertEquals(result.schema("features").metadata.getMetadata("ml_attr").getLong("num_attrs"),1L)
        } finally clean(path)
      }
    } finally clean(directory)
  }

  test("safe local publication rejects existing files, directories, missing parents and unsupported types without changing originals") {
    val directory=Files.createTempDirectory("datacraft-parquet-safe-")
    val path=directory.resolve("existing.parquet")
    try {
      Files.writeString(path,"original")
      intercept[IllegalArgumentException](ParquetIO.writeLocal(table(),config(path)))
      assertEquals(Files.readString(path),"original")
      val overwrite=config(path);overwrite("overwrite")=ujson.Bool(true)
      val invalid=spark.range(1).selectExpr("cast(null as void) as unsupported")
      intercept[Exception](ParquetIO.writeLocal(invalid,overwrite))
      assertEquals(Files.readString(path),"original")
      ParquetIO.writeLocal(table(),overwrite)
      assertEquals(spark.read.parquet(path.toString).count(),2L)
      intercept[IllegalArgumentException](ParquetIO.writeLocal(table(),config(directory)))
      intercept[IllegalArgumentException](ParquetIO.writeLocal(table(),config(directory.resolve("absent/out.parquet"))))
    } finally {clean(path);clean(directory)}
  }

  test("empty datasets produce a valid file and explicit parent creation works") {
    val directory=Files.createTempDirectory("datacraft-parquet-empty-")
    val nested=directory.resolve("nested");val path=nested.resolve("empty.parquet")
    try {
      val settings=config(path);settings("create_parents")=ujson.Bool(true)
      ParquetIO.writeLocal(table().limit(0),settings)
      val result=spark.read.parquet(path.toString);assertEquals(result.count(),0L)
      assertEquals(result.schema.fieldNames.toSeq,table().schema.fieldNames.toSeq)
    } finally {clean(path);clean(nested);clean(directory)}
  }

  test("worker wire JSON preserves int64 and Decimal precision and never exposes private staging fields") {
    val directory=Files.createTempDirectory("datacraft-parquet-wire-");val path=directory.resolve("input.parquet")
    try {
      ParquetIO.writeLocal(table(),config(path))
      val source="00000000-0000-0000-0000-000000000001";val preview="00000000-0000-0000-0000-000000000002"
      val request=Obj("protocol_version"->1,"request_id"->"00000000-0000-0000-0000-000000000099","operation"->"run",
        "output_node"->preview,"execution_order"->ujson.Arr(source,preview),"project"->Obj("format_version"->1,
          "nodes"->ujson.Arr(Obj("id"->source,"config"->Obj("kind"->"parquet_source","path"->path.toString)),
            Obj("id"->preview,"config"->Obj("kind"->"table_preview","rows"->10))),
          "edges"->ujson.Arr(Obj("from"->source,"from_port"->"table","to"->preview,"to_port"->"table"))))
      val response=Worker.execute(spark,request);assert(response("ok").bool,ujson.write(response))
      val wire=Worker.encodeResponse(response)
      val json=new com.fasterxml.jackson.databind.ObjectMapper().enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_DECIMAL_FOR_FLOATS).readTree(wire)
      val rows=json.get("tables").get(0).get("rows")
      val precise=(0 until rows.size()).map(rows.get).find(_.get("id").longValue()==large).get
      assertEquals(precise.get("amount").decimalValue().compareTo(new java.math.BigDecimal("12345678901234567890.1234567890")),0)
      assert(wire.contains("12345678901234567890.1234567890"))
      assert(wire.contains("9007199254740993"));assert(!wire.contains("__raw_rows"));assert(!wire.contains("__raw_schema"))
      assert(rows.get(0).has("amount"))
    } finally {clean(path);clean(directory)}
  }
}
