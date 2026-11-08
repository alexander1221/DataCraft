package datacraft

import java.nio.file.{Files, Paths}
import org.apache.spark.ml.Transformer
import org.apache.spark.ml.feature.VectorAssembler
import org.apache.spark.ml.linalg.Vector
import org.apache.spark.sql.{DataFrame, SparkSession}
import org.apache.spark.sql.functions.lit
import ujson.{Arr, Obj, Value}

class ClassificationSuite extends munit.FunSuite {
  private var spark: SparkSession = _
  private val Kinds = Seq("logistic_regression","linear_svc","decision_tree_classifier","random_forest_classifier")
  override def beforeAll(): Unit = {
    spark = SparkSession.builder().master("local[2]").appName("DataCraft classification tests")
      .config("spark.ui.enabled","false").config("spark.driver.host","127.0.0.1")
      .config("spark.driver.bindAddress","127.0.0.1").config("spark.sql.shuffle.partitions","2").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }
  override def afterAll(): Unit = if (spark != null) spark.stop()
  private def id(i: Int): String = f"00000000-0000-0000-0000-$i%012d"
  private def request(kind: String, preview: Boolean = false): Value = {
    val file = Paths.get("../../examples/pipelines",kind+".dcraft").toAbsolutePath.normalize()
    val project = ujson.read(Files.readString(file))
    project("nodes")(0)("config")("path") = file.getParent.resolve(project("nodes")(0)("config")("path").str).normalize().toString
    val order = if (preview) (1 to 9) else (1 to 7) :+ 10
    Obj("protocol_version" -> 1,"request_id" -> id(99),"operation" -> "run","project" -> project,
      "output_node" -> id(if (preview) 9 else 10),"execution_order" -> Arr.from(order.map(id)))
  }
  private def config(kind: String): Value = request(kind)("project")("nodes")(4)("config")
  private def features(df: DataFrame): DataFrame =
    new VectorAssembler().setInputCols(Array("x")).setOutputCol("features").transform(df)
  private def predictions(model: Transformer, df: DataFrame): Seq[Double] =
    model.transform(df).select("prediction").limit(20).collect().map(_.getDouble(0)).toSeq

  test("four classifiers predict exact held-out classes and metrics via protocol") {
    for (kind <- Kinds) {
      val result = Worker.execute(spark,request(kind,true))
      assert(result("ok").bool,ujson.write(result))
      val rows = result("tables")(0)("rows").arr
      assertEquals(rows.map(_("id").num).toSeq,Seq(9.0,10.0,11.0,12.0))
      assertEquals(rows.map(_("prediction").num).toSeq,Seq(0.0,0.0,1.0,1.0))
      val metrics = Worker.execute(spark,request(kind))
      assert(metrics("ok").bool,ujson.write(metrics))
      assertEquals(metrics("tables")(0)("name").str,"Classification metrics")
      assertEquals(metrics("tables")(0)("schema")("fields")(1)("type").str,"double")
      assertEquals(metrics("tables")(0)("rows").arr.map(_("value").num).toSeq,Seq.fill(4)(1.0))
    }
  }

  test("logistic probability threshold is applied and SVC exposes margins without fake probabilities") {
    val session = spark; import session.implicits._
    val train = features(Seq((-3.0,0.0),(-1.0,0.0),(1.0,1.0),(3.0,1.0)).toDF("x","label"))
    val test = features(Seq(-2.0,2.0).toDF("x"))
    val cfg = config("logistic_regression")
    cfg("threshold")=0.0
    val low = ClassificationSupport.fit(train,cfg)
    assertEquals(predictions(low,test),Seq(1.0,1.0))
    cfg("threshold")=1.0
    val high = ClassificationSupport.fit(train,cfg)
    assertEquals(predictions(high,test),Seq(0.0,0.0))
    high.transform(test).select("probability").limit(10).collect().foreach { row =>
      val probability = row.getAs[Vector](0).toArray
      assertEquals(probability.length,2)
      assert(probability.forall(p => p.isFinite && p>=0 && p<=1))
    }
    val svm = ClassificationSupport.fit(train,config("linear_svc"))
    assert(ClassificationSupport.isClassifier(svm))
    assert(!RegressionSupport.isRegression(svm))
    val transformed = svm.transform(test)
    assert(transformed.columns.contains("rawPrediction"))
    assert(!transformed.columns.contains("probability"))
    assertEquals(predictions(svm,test),Seq(0.0,1.0))
  }

  test("tree and forest support three classes while binomial estimators reject them") {
    val session = spark; import session.implicits._
    val train = features(Seq((0.0,0.0),(1.0,0.0),(4.0,1.0),(5.0,1.0),(8.0,2.0),(9.0,2.0)).toDF("x","label"))
    val test = features(Seq(0.5,4.5,8.5).toDF("x"))
    for (kind <- Seq("decision_tree_classifier","random_forest_classifier")) {
      val cfg=config(kind); cfg("max_depth")=2; cfg("impurity")="entropy"
      val model=ClassificationSupport.fit(train,cfg)
      assertEquals(predictions(model,test),Seq(0.0,1.0,2.0))
    }
    for (kind <- Seq("logistic_regression","linear_svc")) {
      assert(intercept[IllegalArgumentException](ClassificationSupport.fit(train,config(kind))).getMessage.contains("binary"))
    }
  }

  test("weighted metrics match exact symmetric and absent-class oracles") {
    val session = spark; import session.implicits._
    val df=Seq((0.0,0.0),(0.0,1.0),(1.0,0.0),(1.0,1.0)).toDF("label","prediction")
    val expected=Seq("accuracy"->0.5,"weighted_precision"->0.5,"weighted_recall"->0.5,"weighted_f1"->0.5)
    assertEquals(ClassificationSupport.metrics(df,"label","prediction"),expected)
    assertEquals(ClassificationSupport.metrics(df.repartition(3),"label","prediction"),expected)
    val perfect=Seq((0.0,0.0),(1.0,1.0),(2.0,2.0)).toDF("label","prediction")
    assertEquals(ClassificationSupport.metrics(perfect,"label","prediction").map(_._2),Seq.fill(4)(1.0))
    val manyPerfect=spark.range(0,1000).selectExpr("cast(id as double) as label","cast(id as double) as prediction")
    assertEquals(ClassificationSupport.metrics(manyPerfect,"label","prediction").map(_._2),Seq.fill(4)(1.0))
    val wrong=Seq((0.0,1.0),(0.0,1.0)).toDF("label","prediction")
    assertEquals(ClassificationSupport.metrics(wrong,"label","prediction").map(_._2),Seq.fill(4)(0.0))
    val dotted=df.withColumnRenamed("label","class.name").withColumnRenamed("prediction","pred.name")
    assertEquals(ClassificationSupport.metrics(dotted,"class.name","pred.name"),expected)
  }

  test("metrics reject invalid IDs empty inputs schemas and unbounded confusion cardinality") {
    val session = spark; import session.implicits._
    val valid=Seq((0.0,1.0)).toDF("label","prediction")
    intercept[IllegalArgumentException](ClassificationSupport.metrics(valid.limit(0),"label","prediction"))
    intercept[IllegalArgumentException](ClassificationSupport.metrics(valid,"label","label"))
    intercept[IllegalArgumentException](ClassificationSupport.metrics(valid,"absent","prediction"))
    for (bad <- Seq(-1.0,0.5,Double.NaN,Double.PositiveInfinity,Double.NegativeInfinity,2147483648.0)) {
      intercept[IllegalArgumentException](ClassificationSupport.metrics(Seq((bad,0.0)).toDF("label","prediction"),"label","prediction"))
      intercept[IllegalArgumentException](ClassificationSupport.metrics(Seq((0.0,bad)).toDF("label","prediction"),"label","prediction"))
    }
    intercept[IllegalArgumentException](ClassificationSupport.metrics(Seq((None:Option[Double],Some(1.0))).toDF("label","prediction"),"label","prediction"))
    intercept[IllegalArgumentException](ClassificationSupport.metrics(Seq(("0",1.0)).toDF("label","prediction"),"label","prediction"))
    val many=spark.range(0,1001).selectExpr("cast(id as double) as label","cast(id as double) as prediction")
    assert(intercept[IllegalArgumentException](ClassificationSupport.metrics(many,"label","prediction")).getMessage.contains("1000"))
  }

  test("invalid training labels schema collisions and protocol errors are recoverable") {
    val session = spark; import session.implicits._
    val cfg=config("decision_tree_classifier")
    val train=features(Seq((0.0,0.0),(1.0,1.0)).toDF("x","label"))
    for (labels <- Seq(Seq(0.0,2.0),Seq(-1.0,1.0),Seq(0.0,0.5),Seq(0.0,Double.NaN),Seq(0.0,Double.PositiveInfinity),Seq(1.0,1.0))) {
      val invalid=features(labels.zipWithIndex.map {case (y,x)=>(x.toDouble,y)}.toDF("x","label"))
      intercept[IllegalArgumentException](ClassificationSupport.fit(invalid,cfg))
    }
    intercept[IllegalArgumentException](ClassificationSupport.fit(train.limit(0),cfg))
    intercept[IllegalArgumentException](ClassificationSupport.fit(train.withColumn("probability",lit(0)),cfg))
    val many=features(spark.range(0,1001).selectExpr("cast(id as double) as x","cast(id as double) as label"))
    intercept[IllegalArgumentException](ClassificationSupport.fit(many,cfg))
    val wrong=request("logistic_regression")
    wrong("project")("nodes")(6)("config")("kind")="transform"
    assert(Worker.execute(spark,wrong)("message").str.contains("Transform requires a preprocessing model"))
    val invalid=request("decision_tree_classifier")
    invalid("project")("nodes")(4)("config")("impurity")="variance"
    val result=Worker.execute(spark,invalid)
    assert(!result("ok").bool)
    assertEquals(result("node").str,id(5))
    assert(Worker.execute(spark,request("decision_tree_classifier"))("ok").bool)
  }

  test("changing held-out labels cannot retrain a classifier or hide its errors") {
    val path=Files.createTempFile("datacraft-classification-holdout-",".jsonl")
    try {
      val records=Files.readString(Paths.get("../../examples/data/classification.jsonl")).linesIterator.map(ujson.read(_)).toSeq
      records.filter(_("id").num>=9).foreach(r=>r("target")=1-r("target").num)
      Files.writeString(path,records.map(ujson.write(_)).mkString("\n")+"\n")
      val preview=request("random_forest_classifier",true)
      preview("project")("nodes")(0)("config")("path")=path.toString
      val predicted=Worker.execute(spark,preview)
      assert(predicted("ok").bool,ujson.write(predicted))
      assertEquals(predicted("tables")(0)("rows").arr.map(_("prediction").num).toSeq,Seq(0.0,0.0,1.0,1.0))
      val req=request("random_forest_classifier")
      req("project")("nodes")(0)("config")("path")=path.toString
      val result=Worker.execute(spark,req)
      assert(result("ok").bool,ujson.write(result))
      assertEquals(result("tables")(0)("rows").arr.map(_("value").num).toSeq,Seq.fill(4)(0.0))
    } finally Files.delete(path)
  }

  test("bootstrap classification forests repeat configured-seed predictions") {
    val session = spark; import session.implicits._
    val train=features((0 until 32).map(i=>(i.toDouble,(i%2).toDouble)).toDF("x","label"))
    val cfg=config("random_forest_classifier"); cfg("bootstrap")=true; cfg("num_trees")=5
    val test=features(Seq(1.0,12.0,25.0).toDF("x"))
    val a=predictions(ClassificationSupport.fit(train,cfg),test)
    val b=predictions(ClassificationSupport.fit(train,cfg),test)
    assertEquals(a,b)
    assert(a.forall(v=>v==0.0 || v==1.0))
  }
}
