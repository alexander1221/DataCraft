package datacraft

import java.nio.file.{Files, Paths}
import org.apache.parquet.example.data.simple.SimpleGroupFactory
import org.apache.parquet.hadoop.example.ExampleParquetWriter
import org.apache.parquet.io.LocalOutputFile
import org.apache.parquet.schema.MessageTypeParser
import org.apache.spark.sql.SparkSession
import ujson.{Arr, Obj, Value}

class WorkerSuite extends munit.FunSuite {
  private var spark: SparkSession = _

  override def beforeAll(): Unit = {
    spark = SparkSession.builder().master("local[2]").appName("DataCraft tests")
      .config("spark.ui.enabled", "false")
      .config("spark.driver.host", "127.0.0.1")
      .config("spark.driver.bindAddress", "127.0.0.1")
      .config("spark.sql.shuffle.partitions", "2").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }

  override def afterAll(): Unit = if (spark != null) spark.stop()

  private def request(file: String): Value = {
    val path = Paths.get("../../examples/pipelines", file).toAbsolutePath.normalize()
    val project = ujson.read(Files.readString(path))
    project("nodes").arr.foreach { node =>
      val config = node("config")
      if (config.obj.contains("path")) {
        config("path") = path.getParent.resolve(config("path").str).normalize().toString
      }
    }
    Obj("protocol_version" -> 1, "request_id" -> "00000000-0000-0000-0000-000000000099",
      "operation" -> "run", "project" -> project,
      "output_node" -> project("output_node"),
      "execution_order" -> Arr.from(project("nodes").arr.map(_("id"))))
  }

  test("CSV filter projection limit preview returns exact rows and schema") {
    val result = Worker.execute(spark, request("sample.dcraft"))
    assert(result("ok").bool, ujson.write(result))
    val table = result("tables")(0)
    val rows = table("rows").arr
    assertEquals(rows.size, 3)
    assertEquals(rows.map(_("quantity").num).toSeq, Seq(4.0, 5.0, 6.0))
    assertEquals(table("schema")("fields").arr.map(_("name").str).toSeq, Seq("quantity", "amount"))
  }

  test("linear regression recovers known coefficients and reports training metrics") {
    val result = Worker.execute(spark, request("regression.dcraft"))
    assert(result("ok").bool, ujson.write(result))
    val estimates = result("tables")(0)("rows").arr.map(r => r("parameter").str -> r("estimate").num).toMap
    assert(math.abs(estimates("Intercept") - 3.0) < 1e-6)
    assert(math.abs(estimates("feature[0]") - 2.0) < 1e-6)
    val metrics = result("tables")(1)("rows").arr.map(r => r("metric").str -> r("value").num).toMap
    assert(metrics("rmse") < 1e-6)
  }

  test("prediction uses fitted model") {
    val result = Worker.execute(spark, request("prediction.dcraft"))
    assert(result("ok").bool, ujson.write(result))
    result("tables")(0)("rows").arr.foreach { row =>
      assert(math.abs(row("prediction").num - row("amount").num) < 1e-6)
    }
  }

  test("unknown column returns a recoverable node error") {
    val req = request("sample.dcraft")
    req("project")("nodes")(2)("config")("columns") = Arr("does_not_exist")
    val result = Worker.execute(spark, req)
    assert(!result("ok").bool)
    assert(result("message").str.contains("Unknown column"))
    assertEquals(result("node").str, req("project")("nodes")(2)("id").str)
    assert(Worker.execute(spark, request("sample.dcraft"))("ok").bool)
  }

  test("missing file and invalid filter report errors") {
    val missing = request("sample.dcraft")
    missing("project")("nodes")(0)("config")("path") = Paths.get("absent-datacraft-test-file.csv").toAbsolutePath.toString
    assert(!Worker.execute(spark, missing)("ok").bool)
    val invalid = request("sample.dcraft")
    invalid("project")("nodes")(1)("config")("expression") = "quantity > >"
    assert(!Worker.execute(spark, invalid)("ok").bool)
  }

  test("JSON training-only categorical model transforms held-out and unseen categories") {
    val result = Worker.execute(spark, request("index_categoricals.dcraft"))
    assert(result("ok").bool, ujson.write(result))
    assertEquals(result("tables")(0)("rows"), Arr(
      Obj("id" -> 5, "category" -> "a", "category_idx" -> 0.0),
      Obj("id" -> 6, "category" -> "c", "category_idx" -> 2.0)))
    val strict = request("index_categoricals.dcraft")
    strict("project")("nodes")(3)("config")("keep_unknown") = false
    assert(!Worker.execute(spark, strict)("ok").bool)
    val wrong = request("index_categoricals.dcraft")
    wrong("project")("nodes")(4)("config")("kind") = "predict"
    val error = Worker.execute(spark, wrong)
    assert(!error("ok").bool)
    assert(error("message").str.contains("Predict requires a regression model"))
  }

  test("malformed NDJSON returns a source error without poisoning the next request") {
    val path = Files.createTempFile("datacraft-invalid-json-", ".jsonl")
    try {
      Files.writeString(path, "{invalid json\n")
      val req = request("index_categoricals.dcraft")
      req("project")("nodes")(0)("config")("path") = path.toString
      val error = Worker.execute(spark, req)
      assert(!error("ok").bool)
      assertEquals(error("node"), req("project")("nodes")(0)("id"))
      assert(Worker.execute(spark, request("index_categoricals.dcraft"))("ok").bool)
    } finally Files.delete(path)
  }

  private def vector(value: Value): Seq[Double] = {
    val values = value("values").arr.map(_.num).toSeq
    if (value("type").num == 1) values
    else {
      val entries = value("indices").arr.map(_.num.toInt).zip(values).toMap
      (0 until value("size").num.toInt).map(i => entries.getOrElse(i, 0.0))
    }
  }

  test("one-hot reference coding and numeric-vector assembly execute through the protocol") {
    val result = Worker.execute(spark, request("reference_code.dcraft"))
    assert(result("ok").bool, ujson.write(result))
    val rows = result("tables")(0)("rows").arr
    assertEquals(rows.map(_("id").num).toSeq, Seq(5.0, 6.0))
    assertEquals(vector(rows(0)("category_idx_vec")), Seq(1.0, 0.0))
    assertEquals(vector(rows(1)("category_idx_vec")), Seq(0.0, 0.0))
    assertEquals(vector(rows(0)("features")), Seq(5.0, 1.0, 0.0))
    assertEquals(vector(rows(1)("features")), Seq(6.0, 0.0, 0.0))
  }

  test("RFormula fits training rows and reports recoverable invalid formula and empty fit") {
    val result = Worker.execute(spark, request("design_matrix.dcraft"))
    assert(result("ok").bool, ujson.write(result))
    assertEquals(result("tables")(0)("rows"), Arr(
      Obj("id" -> 5, "x" -> 5, "y" -> 13, "label" -> 13.0),
      Obj("id" -> 6, "x" -> 6, "y" -> 15, "label" -> 15.0)))
    val invalid = request("design_matrix.dcraft")
    invalid("project")("nodes")(3)("config")("formula") = "y ~ ~ x"
    val error = Worker.execute(spark, invalid)
    assert(!error("ok").bool)
    assertEquals(error("node"), invalid("project")("nodes")(3)("id"))
    val empty = request("design_matrix.dcraft")
    empty("project")("nodes")(1)("config")("expression") = "id < 0"
    assert(!Worker.execute(spark, empty)("ok").bool)
    val regression = request("prediction.dcraft")
    regression("project")("nodes")(4)("config")("kind") = "transform"
    assert(Worker.execute(spark, regression)("message").str.contains("Transform requires a preprocessing model"))
  }

  test("split output ports select distinct branches without leaking training rows") {
    val req = request("train_test.dcraft")
    val test = Worker.execute(spark, req)
    assert(test("ok").bool, ujson.write(test))
    val trainReq = request("train_test.dcraft")
    val testEdge = trainReq("project")("edges").arr.find(_("from_port").str == "test").get
    testEdge("from_port") = "train"
    val train = Worker.execute(spark, trainReq)
    assert(train("ok").bool, ujson.write(train))
    def ids(result: Value) = result("tables")(0)("rows").arr.map(_("id").num.toInt).toSet
    assert(ids(train).nonEmpty && ids(test).nonEmpty)
    assertEquals(ids(train).intersect(ids(test)), Set.empty[Int])
    assertEquals(ids(train) ++ ids(test), (1 to 128).toSet)
    test("tables")(0)("rows").arr.foreach(row => assertEquals(vector(row("features")), Seq(row("x").num)))
  }

  test("Parquet source executes with schema preserved") {
    val directory = Files.createTempDirectory("datacraft-parquet-")
    try {
      val req = request("sample.dcraft")
      val path = directory.resolve("sales.parquet")
      // This tests a source, not Spark export. Create an independent fixture
      // through Java NIO, avoiding Hadoop's Windows permission/winutils path.
      val schema = MessageTypeParser.parseMessageType(
        "message sales { required int32 quantity; required int32 amount; required binary region (UTF8); }")
      val factory = new SimpleGroupFactory(schema)
      val writer = ExampleParquetWriter.builder(new LocalOutputFile(path)).withType(schema).build()
      try {
        (1 to 12).foreach { quantity =>
          writer.write(factory.newGroup().append("quantity", quantity)
            .append("amount", 2 * quantity + 3).append("region", "EU"))
        }
      } finally writer.close()
      req("project")("nodes")(0)("config") = Obj("kind" -> "parquet_source", "path" -> path.toString)
      val result = Worker.execute(spark, req)
      assert(result("ok").bool, ujson.write(result))
      val table = result("tables")(0)
      assertEquals(table("rows").arr.map(_("quantity").num).toSeq, Seq(4.0, 5.0, 6.0))
      assertEquals(table("rows").arr.map(_("amount").num).toSeq, Seq(11.0, 13.0, 15.0))
      assertEquals(table("schema")("fields").arr.map(f => f("name").str -> f("type").str).toSeq,
        Seq("quantity" -> "integer", "amount" -> "integer"))
    } finally {
      val files = Files.walk(directory)
      try {
        import scala.jdk.CollectionConverters._
        files.iterator().asScala.toSeq.sortBy(_.getNameCount).reverse.foreach(p => Files.delete(p))
      } finally files.close()
    }
  }
}
