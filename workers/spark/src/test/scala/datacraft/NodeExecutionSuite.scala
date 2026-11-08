package datacraft

import java.nio.file.{Files, Paths}
import org.apache.spark.sql.SparkSession
import ujson.{Arr, Obj, Str, Value}

class NodeExecutionSuite extends munit.FunSuite {
  private var spark: SparkSession = _
  private def id(n: Int): String = f"00000000-0000-0000-0000-$n%012d"
  override def beforeAll(): Unit = {
    spark = SparkSession.builder().master("local[2]").appName("DataCraft cached node tests")
      .config("spark.ui.enabled", "false").config("spark.driver.host", "127.0.0.1")
      .config("spark.driver.bindAddress", "127.0.0.1").config("spark.sql.shuffle.partitions", "2").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }
  override def afterAll(): Unit = if (spark != null) spark.stop()
  private def request(file: String): Value = {
    val path = Paths.get("../../examples/pipelines", file).toAbsolutePath.normalize()
    val project = ujson.read(Files.readString(path))
    project("nodes").arr.foreach { n =>
      val c = n("config")
      if (c.obj.contains("path")) c("path") = Str(path.getParent.resolve(c("path").str).normalize().toString)
    }
    Obj("protocol_version" -> 1, "request_id" -> id(99), "operation" -> "run_nodes",
      "project" -> project, "output_node" -> project("output_node"), "progress" -> true,
      "cache_tokens" -> Obj.from(project("nodes").arr.map(n => n("id").str -> Str("token-" + n("id").str))))
  }
  private def run(req: Value, cache: Worker.Cache, nodes: Seq[String], events: scala.collection.mutable.Buffer[Value]): Value = {
    req("execution_order") = Arr.from(nodes.map(Str(_)))
    req("output_node") = Str(nodes.last)
    Worker.execute(spark, req, cache, e => {events += e; ()})
  }

  test("materialized source survives file removal; strict filter does not reread it; rerun reports source error") {
    val file = Files.createTempFile("datacraft-cache-", ".csv")
    Files.writeString(file, "quantity,amount\n1,5\n4,11\n5,13\n6,15\n")
    val req = request("sample.dcraft")
    req("project")("nodes")(0)("config")("path") = Str(file.toString)
    val cache = new Worker.Cache
    val events = scala.collection.mutable.Buffer.empty[Value]
    try {
      assert(run(req, cache, Seq(id(1)), events)("ok").bool)
      assert(cache.tables((id(1), "table")).storageLevel.useDisk)
      Files.delete(file)
      val filtered = run(req, cache, Seq(id(2)), events)
      assert(filtered("ok").bool, ujson.write(filtered))
      assertEquals(filtered("tables")(0)("rows").arr.map(_("quantity").num).toSeq, Seq(4.0,5.0,6.0))
      assertEquals(events.map(e => (e("node").str,e("phase").str)).toSeq,
        Seq((id(1),"running"),(id(1),"completed"),(id(2),"running"),(id(2),"completed")))
      val missing = run(req, cache, Seq(id(1)), events)
      assert(!missing("ok").bool)
      assertEquals(missing("node").str, id(1))
    } finally { Files.deleteIfExists(file); cache.tokens.keys.toVector.foreach(cache.remove) }
  }

  test("cached fitted model is reused by prediction; changed tokens evict invalid dependencies") {
    val req = request("prediction.dcraft")
    val cache = new Worker.Cache
    val events = scala.collection.mutable.Buffer.empty[Value]
    val nodes = req("project")("nodes").arr.map(_("id").str).toSeq
    try {
      val first = run(req, cache, nodes, events)
      assert(first("ok").bool, ujson.write(first))
      val predict = req("project")("nodes").arr.find(_("config")("kind").str == "predict").get("id").str
      val second = run(req, cache, Seq(predict), events)
      assert(second("ok").bool, ujson.write(second))
      second("tables")(0)("rows").arr.foreach(r => assert(math.abs(r("prediction").num-r("amount").num) < 1e-6))
      // Provisional 1e-6 solver acceptance from WorkerSuite; not a solver error bound.
      req("cache_tokens") = Obj()
      assert(!run(req, cache, Seq(predict), events)("ok").bool)
      assert(cache.models.isEmpty)
    } finally { cache.tokens.keys.toVector.foreach(cache.remove) }
  }

  test("whole branched feature-selection DAG materializes train and test branches and executes terminal reports") {
    val req = request("random_forest_pipeline.dcraft")
    val cache = new Worker.Cache
    val events = scala.collection.mutable.Buffer.empty[Value]
    val nodes = req("project")("nodes").arr.map(_("id").str).toSeq
    try {
      val result = run(req, cache, nodes, events)
      assert(result("ok").bool, ujson.write(result))
      val metrics = result("tables").arr.find(_("name").str.startsWith("Classification metrics")).get
      // Independent fixture rule: signal <= 4 has target 0, signal >= 5 has target 1.
      assertEquals(metrics("rows").arr.map(r => r("metric").str -> r("value").num).toMap,
        Map("accuracy" -> 1.0,"weighted_precision" -> 1.0,"weighted_recall" -> 1.0,"weighted_f1" -> 1.0))
      assert(cache.tables((id(2),"table")).storageLevel.useDisk)
      assert(cache.tables((id(3),"table")).storageLevel.useDisk)
      assertEquals(events.count(_("phase").str == "completed"), nodes.size)
    } finally {cache.tokens.keys.toVector.foreach(cache.remove)}
  }
  test("train-test split caches both typed outputs") {
    val req = request("train_test.dcraft")
    val cache = new Worker.Cache
    val events = scala.collection.mutable.Buffer.empty[Value]
    val source = req("project")("nodes").arr.find(_("config")("kind").str == "json_source")
      .orElse(req("project")("nodes").arr.find(_("config")("kind").str == "csv_source")).get("id").str
    val split = req("project")("nodes").arr.find(_("config")("kind").str == "train_test_split").get("id").str
    try {
      val result = run(req,cache,Seq(source,split),events)
      assert(result("ok").bool,ujson.write(result))
      assert(cache.tables((split,"train")).storageLevel.useDisk)
      assert(cache.tables((split,"test")).storageLevel.useDisk)
      val sizes=Seq("train","test").map(p=>cache.tables((split,p)).count())
      assertEquals(sizes.sum,cache.tables((source,"table")).count())
    } finally {cache.tokens.keys.toVector.foreach(cache.remove)}
  }
}
