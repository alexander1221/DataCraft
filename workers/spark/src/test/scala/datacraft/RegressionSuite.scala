package datacraft

import java.nio.file.{Files, Paths}
import org.apache.spark.ml.feature.VectorAssembler
import org.apache.spark.sql.{DataFrame, SparkSession}
import ujson.{Arr, Obj, Value}

class RegressionSuite extends munit.FunSuite {
  private var spark: SparkSession = _
  // ASSUMPTION: provisional solver acceptance from docs/REGRESSION.md; not an error bound.
  private val SolverAcceptance = 1e-6
  override def beforeAll(): Unit = {
    spark = SparkSession.builder().master("local[2]").appName("DataCraft regression tests")
      .config("spark.ui.enabled", "false").config("spark.driver.host", "127.0.0.1")
      .config("spark.driver.bindAddress", "127.0.0.1").config("spark.sql.shuffle.partitions", "2").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }
  override def afterAll(): Unit = if (spark != null) spark.stop()
  private def id(i: Int): String = f"00000000-0000-0000-0000-$i%012d"
  private def request(name: String, preview: Boolean = false): Value = {
    val file = Paths.get("../../examples/pipelines", name + ".dcraft").toAbsolutePath.normalize()
    val project = ujson.read(Files.readString(file))
    project("nodes").arr.foreach { n =>
      if (n("config").obj.contains("path"))
        n("config")("path") = file.getParent.resolve(n("config")("path").str).normalize().toString
    }
    // Independent dependency closures for these hand-authored ten-node fixtures.
    val order = if (preview) (1 to 9) else (1 to 7) :+ 10
    Obj("protocol_version" -> 1, "request_id" -> id(99), "operation" -> "run", "project" -> project,
      "output_node" -> id(if (preview) 9 else 10), "execution_order" -> Arr.from(order.map(id)))
  }
  private def features(df: DataFrame): DataFrame =
    new VectorAssembler().setInputCols(Array("x")).setOutputCol("features").transform(df)
  private def glm(family: String): Value = Obj("kind" -> "generalized_linear_regression",
    "label_column" -> "label", "features_column" -> "features", "family" -> family,
    "link" -> "default", "fit_intercept" -> true, "max_iter" -> 25, "reg_param" -> 0.0)

  test("GLM tree and forest fit only train and predict held-out rows through real protocol") {
    for ((name, expected, tolerance) <- Seq(
      ("glm_regression", Seq(13.0,15.0), SolverAcceptance),
      ("decision_tree", Seq(2.0,10.0), 0.0), ("random_forest", Seq(2.0,10.0), 0.0))) {
      val result = Worker.execute(spark, request(name, true))
      assert(result("ok").bool, ujson.write(result))
      val rows = result("tables")(0)("rows").arr
      assertEquals(rows.map(_("id").num).toSeq, Seq(5.0,6.0))
      rows.zip(expected).foreach { case (row, prediction) =>
        assert(math.abs(row("prediction").num - prediction) <= tolerance)
      }
      val evaluated = Worker.execute(spark, request(name))
      assert(evaluated("ok").bool, ujson.write(evaluated))
      assertEquals(evaluated("tables")(0)("schema")("fields")(1)("type").str, "double")
      val metrics = evaluated("tables")(0)("rows").arr.map(r => r("metric").str -> r("value").num).toMap
      Seq("rmse","mse","mae").foreach(m => assert(math.abs(metrics(m)) <= tolerance))
      assert(math.abs(metrics("r2") - 1) <= tolerance)
      // Restricting test membership must not refit or alter the trained model.
      val changed = request(name, true)
      changed("project")("nodes")(2)("config") = Obj("kind" -> "filter", "expression" -> "id = 6")
      val last = Worker.execute(spark, changed)
      assert(last("ok").bool, ujson.write(last))
      assertEquals(last("tables")(0)("rows").arr.size, 1)
      assert(math.abs(last("tables")(0)("rows")(0)("prediction").num - expected.last) <= tolerance)
    }
  }

  test("GLM supported family defaults recover known constant responses") {
    val session = spark; import session.implicits._
    for ((family, mean) <- Seq("gaussian" -> 7.0, "binomial" -> 0.5, "poisson" -> 2.0, "gamma" -> 2.0)) {
      val train = features(Seq((-1.0,mean),(0.0,mean),(1.0,mean),(2.0,mean)).toDF("x","label"))
      val model = RegressionSupport.fit(train, glm(family))
      assert(RegressionSupport.isRegression(model))
      model.transform(features(Seq(3.0,4.0).toDF("x"))).select("prediction").limit(10).collect()
        .foreach(row => assert(math.abs(row.getDouble(0) - mean) <= SolverAcceptance, s"family=$family"))
    }
  }

  test("metrics match an exact residual oracle and undefined R2 remains null") {
    val session = spark; import session.implicits._
    val df = Seq((1.0,2.0),(3.0,2.0)).toDF("label","prediction")
    assertEquals(RegressionSupport.metrics(df,"label","prediction").toMap,
      Map("rmse" -> Some(1.0), "mse" -> Some(1.0), "mae" -> Some(1.0), "r2" -> Some(0.0)))
    val constant = Seq((2.0,2.0),(2.0,2.0)).toDF("label","prediction")
    assertEquals(RegressionSupport.metrics(constant,"label","prediction").toMap.apply("r2"), None)
    val single = Seq((1.0,3.0)).toDF("label","prediction")
    assertEquals(RegressionSupport.metrics(single,"label","prediction").toMap.apply("rmse"), Some(2.0))
    assertEquals(RegressionSupport.metrics(single,"label","prediction").toMap.apply("r2"), None)
    val dotted = df.withColumnRenamed("label","target.name").withColumnRenamed("prediction","predict.name")
    assertEquals(RegressionSupport.metrics(dotted,"target.name","predict.name"), RegressionSupport.metrics(df,"label","prediction"))
    val req = request("decision_tree")
    req("project")("nodes")(2)("config")("expression") = "id = 5"
    val result = Worker.execute(spark, req)
    assert(result("ok").bool, ujson.write(result))
    assertEquals(result("tables")(0)("rows")(3)("value"), ujson.Null)
    assertEquals(result("tables")(0)("schema")("fields")(1)("type").str,"double")
  }

  test("metrics reject empty malformed and nonfinite data rather than dropping rows") {
    val session = spark; import session.implicits._
    val valid = Seq((1.0,2.0)).toDF("label","prediction")
    intercept[IllegalArgumentException](RegressionSupport.metrics(valid.limit(0),"label","prediction"))
    intercept[IllegalArgumentException](RegressionSupport.metrics(valid,"label","label"))
    intercept[IllegalArgumentException](RegressionSupport.metrics(valid,"missing","prediction"))
    for (bad <- Seq(Double.NaN, Double.PositiveInfinity, Double.NegativeInfinity)) {
      intercept[IllegalArgumentException](RegressionSupport.metrics(Seq((bad,2.0)).toDF("label","prediction"),"label","prediction"))
      intercept[IllegalArgumentException](RegressionSupport.metrics(Seq((1.0,bad)).toDF("label","prediction"),"label","prediction"))
    }
    val nullable = Seq((None:Option[Double],Some(2.0))).toDF("label","prediction")
    intercept[IllegalArgumentException](RegressionSupport.metrics(nullable,"label","prediction"))
    intercept[IllegalArgumentException](RegressionSupport.metrics(Seq(("1",2.0)).toDF("label","prediction"),"label","prediction"))
    intercept[IllegalArgumentException](RegressionSupport.metrics(Seq((1e308,-1e308)).toDF("label","prediction"),"label","prediction"))
  }

  test("changing only held-out labels changes metrics but never fits the model again") {
    val path = Files.createTempFile("datacraft-holdout-labels-", ".jsonl")
    try {
      val original = Paths.get("../../examples/data/tree_regression.jsonl")
      val records = Files.readString(original).linesIterator.map(ujson.read(_)).toSeq
      records.filter(_("id").num >= 5).foreach(_("y") = 5)
      Files.writeString(path, records.map(ujson.write(_)).mkString("\n") + "\n")
      val req = request("decision_tree", true)
      req("project")("nodes")(0)("config")("path") = path.toString
      val predicted = Worker.execute(spark, req)
      assert(predicted("ok").bool, ujson.write(predicted))
      assertEquals(predicted("tables")(0)("rows").arr.map(_("prediction").num).toSeq, Seq(2.0,10.0))
      val evaluation = request("decision_tree")
      evaluation("project")("nodes")(0)("config")("path") = path.toString
      val result = Worker.execute(spark, evaluation)
      assert(result("ok").bool, ujson.write(result))
      val values = result("tables")(0)("rows").arr.map(r => r("metric").str -> r("value")).toMap
      assertEquals(values("mse").num,17.0)
      assertEquals(values("mae").num,4.0)
      assertEquals(values("r2"),ujson.Null)
    } finally Files.delete(path)
  }

  test("bootstrap forest uses its configured seed reproducibly within this Spark runtime") {
    val session = spark; import session.implicits._
    val train = features((0 until 32).map(i => (i.toDouble, (i % 7).toDouble)).toDF("x","label"))
    val config = request("random_forest")("project")("nodes")(4)("config")
    config("bootstrap") = true
    config("num_trees") = 5
    val first = RegressionSupport.fit(train,config)
    val second = RegressionSupport.fit(train,config)
    val test = features(Seq(1.0,12.0,25.0).toDF("x"))
    def predictions(model: org.apache.spark.ml.Transformer) =
      model.transform(test).select("prediction").limit(10).collect().map(_.getDouble(0)).toSeq
    val values = predictions(first)
    assert(values.forall(_.isFinite))
    assertEquals(values,predictions(second))
  }

  test("invalid estimator parameters responses schemas and model types return node errors") {
    val session = spark; import session.implicits._
    for ((family, bad) <- Seq("poisson" -> -1.0, "gamma" -> 0.0, "binomial" -> 1.5)) {
      val train = features(Seq((1.0,bad),(2.0,bad)).toDF("x","label"))
      intercept[IllegalArgumentException](RegressionSupport.fit(train, glm(family)))
    }
    for ((file, field, bad) <- Seq(("glm_regression","link",ujson.Str("logit")),
      ("decision_tree","max_depth",ujson.Num(31)), ("random_forest","num_trees",ujson.Num(0)))) {
      val req = request(file)
      req("project")("nodes")(4)("config")(field) = bad
      val result = Worker.execute(spark, req)
      assert(!result("ok").bool)
      assertEquals(result("node").str,id(5))
    }
    val empty = request("decision_tree")
    empty("project")("nodes")(1)("config")("expression") = "id < 0"
    assert(Worker.execute(spark,empty)("message").str.contains("empty"))
    val wrong = request("random_forest")
    wrong("project")("nodes")(6)("config")("kind") = "transform"
    assert(Worker.execute(spark,wrong)("message").str.contains("Transform requires a preprocessing model"))
    val schema = request("glm_regression")
    schema("project")("nodes")(4)("config")("features_column") = "x"
    assert(Worker.execute(spark,schema)("message").str.contains("Spark ML vector"))
    assert(Worker.execute(spark,request("decision_tree"))("ok").bool)
  }
}
