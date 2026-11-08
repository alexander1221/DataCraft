package datacraft

import java.nio.file.{Files,Paths}
import org.apache.spark.ml.attribute.AttributeGroup
import org.apache.spark.ml.linalg.{Vector,Vectors}
import org.apache.spark.ml.param.ParamMap
import org.apache.spark.sql.{DataFrame,SparkSession}
import ujson.{Arr,Obj,Value}

class FeatureSelectionSuite extends munit.FunSuite {
  private var spark:SparkSession=_
  // ASSUMPTION: provisional acceptance, independently derived values; not an error bound.
  private val StatAcceptance=1e-12
  override def beforeAll():Unit={
    spark=SparkSession.builder().master("local[2]").appName("DataCraft feature selection tests")
      .config("spark.ui.enabled","false").config("spark.driver.host","127.0.0.1")
      .config("spark.driver.bindAddress","127.0.0.1").config("spark.sql.shuffle.partitions","2").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }
  override def afterAll():Unit=if(spark!=null)spark.stop()
  private def id(i:Int):String=f"00000000-0000-0000-0000-$i%012d"
  private def request(name:String,output:Int=0):Value={
    val file=Paths.get("../../examples/pipelines",name+".dcraft").toAbsolutePath.normalize()
    val project=ujson.read(Files.readString(file))
    project("nodes").arr.foreach(n=>if(n("config").obj.contains("path"))n("config")("path")=file.getParent.resolve(n("config")("path").str).normalize().toString)
    Obj("protocol_version"->1,"request_id"->id(99),"operation"->"run","project"->project,
      "output_node"->(if(output==0)project("output_node")else ujson.Str(id(output))))
  }
  private def run(req:Value):Value={
    val needed=scala.collection.mutable.Set.empty[String]
    def visit(node:String):Unit=if(needed.add(node))req("project")("edges").arr.filter(_("to").str==node).foreach(e=>visit(e("from").str))
    visit(req("output_node").str)
    req("execution_order")=Arr.from(req("project")("nodes").arr.map(_("id")).filter(n=>needed(n.str)))
    Worker.execute(spark,req)
  }
  private def config(name:String):Value=request(name)("project")("nodes").arr.find(_("config")("kind").str==name).get("config")
  private def vectors(df:DataFrame):Seq[Seq[Double]]=df.select("selected_features").limit(20).collect().map(_.getAs[Vector](0).toArray.toSeq).toSeq
  private def training():DataFrame={
    val session=spark;import session.implicits._
    Seq(("a","u",0.0),("a","v",0.0),("b","u",1.0),("b","v",1.0),("c","u",2.0),("c","v",2.0)).toDF("signal","noise","target")
  }

  test("Pearson report matches independent contingency-table statistic and analytic survival function") {
    val result=run(request("feature_selection"))
    assert(result("ok").bool,ujson.write(result))
    val rows=result("tables")(0)("rows").arr
    assertEquals(rows.map(_("feature").str).toSeq,Seq("signal","noise"))
    assert(math.abs(rows(0)("statistic").num-12.0)<=StatAcceptance)
    assertEquals(rows(0)("degrees_of_freedom").num,4.0)
    assert(math.abs(rows(0)("p_value").num-7*math.exp(-6))<=StatAcceptance)
    assertEquals(rows(1)("statistic").num,0.0)
    assertEquals(rows(1)("degrees_of_freedom").num,2.0)
    assertEquals(rows(1)("p_value").num,1.0)
  }

  test("learned categorical selection works without target and does not learn unseen test labels") {
    val session=spark;import session.implicits._
    val fitted=FeatureSelectionSupport.fit(spark,training(),config("feature_selection_pipeline"))
    val model=fitted.model.get
    val test=Seq(("a","v"),("b","u"),("d","v")).toDF("signal","noise")
    val transformed=model.transform(test)
    assertEquals(vectors(transformed),Seq(Seq(0.0),Seq(1.0),Seq(3.0)))
    assertEquals(transformed.columns.toSeq,Seq("signal","noise","selected_features"))
    assertEquals(AttributeGroup.fromStructField(transformed.schema("selected_features")).attributes.get.map(_.name.get).toSeq,Seq("signal"))
    assertEquals(vectors(model.copy(ParamMap.empty).transform(test)),vectors(transformed))
    val collision=config("feature_selection_pipeline");collision("output_column")="__datacraft_selection_3"
    val collisionModel=FeatureSelectionSupport.fit(spark,training(),collision).model.get
    assertEquals(collisionModel.transform(test).select("__datacraft_selection_3").limit(3).collect().map(_.getAs[Vector](0).toArray.toSeq).toSeq,
      Seq(Seq(0.0),Seq(1.0),Seq(3.0)))
    val result=run(request("feature_selection_pipeline",6))
    assert(result("ok").bool,ujson.write(result))
    assertEquals(result("tables")(0)("rows")(2)("signal").str,"d")
    assertEquals(result("tables")(0)("rows")(2)("selected_features")("values")(0).num,3.0)
  }

  test("continuous values are preserved rather than replaced by bins and constant p-value is undefined") {
    val session=spark;import session.implicits._
    val train=Seq((-3.0,0.0,0.0),(-2.0,0.0,0.0),(-1.0,0.0,0.0),(1.0,0.0,1.0),(2.0,0.0,1.0),(3.0,0.0,1.0)).toDF("signal","noise","target")
    val cfg=config("feature_selection_pipeline");cfg("categorical_columns")=Arr();cfg("num_buckets")=2
    val fitted=FeatureSelectionSupport.fit(spark,train,cfg)
    val report=fitted.report.collect()
    assertEquals(report(1).getLong(2),0L)
    assert(report(1).isNullAt(3))
    val test=Seq((100.0,0.0),(-100.0,0.0)).toDF("signal","noise")
    assertEquals(vectors(fitted.model.get.transform(test)),Seq(Seq(100.0),Seq(-100.0)))
    intercept[IllegalArgumentException](fitted.model.get.transform(Seq((Double.PositiveInfinity,0.0)).toDF("signal","noise")))
    cfg("num_top_features")=2
    assert(intercept[IllegalArgumentException](FeatureSelectionSupport.fit(spark,train,cfg)).getMessage.contains("usable"))
  }

  test("forest importance learns a slicer preserving feature names dimensions and a downstream classifier") {
    val report=run(request("random_forest_pipeline"))
    assert(report("ok").bool,ujson.write(report))
    val rows=report("tables")(0)("rows").arr
    assertEquals(rows.map(_("feature").str).toSeq,Seq("noise","signal"))
    assertEquals(rows.map(_("importance").num).toSeq,Seq(0.0,1.0))
    assertEquals(rows.map(_("selected").bool).toSeq,Seq(false,true))
    val preview=run(request("random_forest_pipeline",8))
    assert(preview("ok").bool,ujson.write(preview))
    assertEquals(preview("tables")(0)("rows").arr.map(_("selected_features")("values")(0).num).toSeq,Seq(1.0,7.0))
    val evaluated=run(request("random_forest_pipeline",11))
    assert(evaluated("ok").bool,ujson.write(evaluated))
    assertEquals(evaluated("tables")(0)("rows").arr.map(_("value").num).toSeq,Seq.fill(4)(1.0))
  }

  test("regression forest selection handles sparse vectors and rejects changed test dimensions") {
    val session=spark;import session.implicits._
    val train=Seq((Vectors.sparse(2,Array(1),Array(0.0)),0.0),(Vectors.sparse(2,Array(1),Array(2.0)),0.0),
      (Vectors.sparse(2,Array(1),Array(6.0)),1.0),(Vectors.sparse(2,Array(1),Array(8.0)),1.0)).toDF("features","label")
    val cfg=config("random_forest_pipeline");cfg("task")="regression"
    val fitted=FeatureSelectionSupport.fit(spark,train,cfg)
    val model=fitted.model.get
    assert(!RegressionSupport.isRegression(model) && !ClassificationSupport.isClassifier(model))
    val test=Seq(Tuple1(Vectors.sparse(2,Array(1),Array(7.0)))).toDF("features")
    assertEquals(vectors(model.transform(test)),Seq(Seq(7.0)))
    val bad=Seq(Tuple1(Vectors.dense(0.0,7.0,1.0))).toDF("features")
    intercept[Exception](model.transform(bad).collect())
    cfg("num_top_features")=3
    intercept[IllegalArgumentException](FeatureSelectionSupport.fit(spark,train,cfg))
    val wrong=request("random_forest_pipeline",11)
    wrong("project")("edges").arr.find(e=>e("to").str==id(10)&&e("to_port").str=="model").get("from")=id(5)
    assert(run(wrong)("message").str.contains("Predict requires"))
  }

  test("equal p-values select by declared order and vector order follows candidate order") {
    val session=spark;import session.implicits._
    val train=Seq(("a","a",0.0),("a","a",0.0),("b","b",1.0),("b","b",1.0)).toDF("second","first","target")
    val cfg=config("feature_selection_pipeline");cfg("columns")=Arr("second","first");cfg("categorical_columns")=Arr("first","second")
    val fitted=FeatureSelectionSupport.fit(spark,train,cfg)
    assertEquals(fitted.report.select("selected").collect().map(_.getBoolean(0)).toSeq,Seq(true,false))
    cfg("num_top_features")=2
    val both=FeatureSelectionSupport.fit(spark,train,cfg).model.get
    val test=Seq(("b","a")).toDF("second","first")
    assertEquals(vectors(both.transform(test)),Seq(Seq(1.0,0.0)))
  }

  test("malformed parameters nulls unknowns schema mismatches and high-cardinality labels fail recoverably") {
    val session=spark;import session.implicits._
    val train=training();val cfg=config("feature_selection_pipeline")
    intercept[IllegalArgumentException](FeatureSelectionSupport.fit(spark,train.limit(0),cfg))
    val nulls=Seq((null:String,"u",0.0),("b","v",1.0)).toDF("signal","noise","target")
    intercept[IllegalArgumentException](FeatureSelectionSupport.fit(spark,nulls,cfg))
    val many=spark.range(0,1001).selectExpr("cast(id as string) as signal","'u' as noise","id as target")
    intercept[IllegalArgumentException](FeatureSelectionSupport.fit(spark,many,cfg))
    cfg("relative_error")= -0.1
    intercept[IllegalArgumentException](FeatureSelectionSupport.fit(spark,train,cfg))
    val model=FeatureSelectionSupport.fit(spark,train,config("feature_selection_pipeline")).model.get
    intercept[IllegalArgumentException](model.transform(Seq(Tuple1("a")).toDF("signal")))
    intercept[Exception](model.transform(train.withColumn("selected_features",org.apache.spark.sql.functions.lit(1))))
    val strict=request("feature_selection_pipeline",6)
    strict("project")("nodes").arr.find(_("id").str==id(4)).get("config")("keep_unknown")=false
    assert(!run(strict)("ok").bool)
    assert(run(request("feature_selection"))("ok").bool)
  }

  test("changing only held-out targets cannot change learned forest selection") {
    val path=Files.createTempFile("datacraft-selection-heldout-",".jsonl")
    try {
      val data=Files.readString(Paths.get("../../examples/data/selection_forest.jsonl")).linesIterator.map(ujson.read(_)).toSeq
      data.filter(_("id").num>=5).foreach(r=>r("target")=1-r("target").num)
      Files.writeString(path,data.map(ujson.write(_)).mkString("\n")+"\n")
      val req=request("random_forest_pipeline",11);req("project")("nodes")(0)("config")("path")=path.toString
      val result=run(req)
      assert(result("ok").bool,ujson.write(result))
      assertEquals(result("tables")(0)("rows").arr.map(_("value").num).toSeq,Seq.fill(4)(0.0))
    } finally Files.delete(path)
  }
}
