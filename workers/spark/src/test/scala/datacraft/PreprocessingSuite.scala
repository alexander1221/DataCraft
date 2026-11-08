package datacraft

import org.apache.spark.ml.feature.StringIndexerModel
import org.apache.spark.ml.linalg.Vector
import org.apache.spark.sql.{DataFrame, SparkSession}
import org.apache.spark.sql.functions.{col, desc}

class PreprocessingSuite extends munit.FunSuite {
  private var spark: SparkSession = _
  override def beforeAll(): Unit = {
    spark = SparkSession.builder().master("local[2]").appName("DataCraft preprocessing tests")
      .config("spark.ui.enabled", "false").config("spark.driver.host", "127.0.0.1")
      .config("spark.driver.bindAddress", "127.0.0.1")
      .config("spark.sql.shuffle.partitions", "2").getOrCreate()
    spark.sparkContext.setLogLevel("ERROR")
  }
  override def afterAll(): Unit = if (spark != null) spark.stop()
  private def vectors(df: DataFrame, name: String): Seq[Seq[Double]] =
    df.select(col(FeaturePreparation.quoted(name))).limit(20).collect().map(_.getAs[Vector](0).toArray.toSeq).toSeq

  test("key hash split is exhaustive disjoint grouped and invariant to row order and partitions") {
    val df = spark.range(0, 128).selectExpr("id", "id % 19 as key")
    def keys(df: DataFrame): Set[Long] = df.select("key").distinct().limit(128).collect().map(_.getLong(0)).toSet
    val (train, test) = FeaturePreparation.split(df, Seq("key"), 0.6, 42)
    val a = keys(train); val b = keys(test)
    assert(a.nonEmpty && b.nonEmpty)
    assertEquals(a.intersect(b), Set.empty[Long])
    assertEquals(a.union(b), (0L until 19L).toSet)
    assertEquals(train.count() + test.count(), 128L)
    assertEquals(train.union(test).exceptAll(df).count(), 0L)
    val (reordered, _) = FeaturePreparation.split(df.repartition(3).orderBy(desc("id")), Seq("key"), 0.6, 42)
    assertEquals(keys(reordered), a)
    val (changed, _) = FeaturePreparation.split(df, Seq("key"), 0.6, 43)
    assert(keys(changed) != a)
    assertEquals(train.schema, df.schema)
    val nullable = df.selectExpr("id", "case when key < 3 then null else key end as key")
    val (nullTrain, nullTest) = FeaturePreparation.split(nullable, Seq("key"), 0.6, 42)
    val leftNull = nullTrain.filter(col("key").isNull).count()
    val rightNull = nullTest.filter(col("key").isNull).count()
    assert(leftNull == 0 || rightNull == 0)
    assertEquals(leftNull + rightNull, nullable.filter(col("key").isNull).count())
    assertEquals(nullTrain.count() + nullTest.count(), 128L)
    intercept[IllegalArgumentException](FeaturePreparation.split(df, Seq("missing"), 0.6, 42))
    intercept[IllegalArgumentException](FeaturePreparation.split(df, Seq("key"), Double.NaN, 42))
  }

  test("indexer fits train vocabulary only and keeps unknown and null in a single extra bucket") {
    val session = spark; import session.implicits._
    val train = Seq("b", "a", "b").toDF("category")
    val model = FeaturePreparation.index(train, Array("category"), true, false, false)
    assertEquals(model.stages.head.asInstanceOf[StringIndexerModel].labelsArray.head.toSeq, Seq("a", "b"))
    val test = Seq("a", "z", null: String).toDF("category")
    val indexed = model.transform(test)
    assertEquals(indexed.select("category_idx").limit(10).collect().map(_.getDouble(0)).toSeq, Seq(0.0, 2.0, 2.0))
    val strict = FeaturePreparation.index(train, Array("category"), false, false, false)
    intercept[Exception](strict.transform(test).limit(10).collect())
    assertEquals(model.stages.head.asInstanceOf[StringIndexerModel].labelsArray.head.toSeq, Seq("a", "b"))
  }

  test("reference coding exact 0/1 vectors and unknown bucket semantics are stable") {
    val session = spark; import session.implicits._
    val train = Seq("a", "b", "a").toDF("category")
    val known = Seq("a", "b").toDF("category")
    val reference = FeaturePreparation.index(train, Array("category"), false, true, true)
    assertEquals(vectors(reference.transform(known), "category_idx_vec"), Seq(Seq(1.0), Seq(0.0)))
    val full = FeaturePreparation.index(train, Array("category"), false, true, false)
    assertEquals(vectors(full.transform(known), "category_idx_vec"), Seq(Seq(1.0,0.0), Seq(0.0,1.0)))
    val keep = FeaturePreparation.index(train, Array("category"), true, true, true)
    val test = Seq("a", "b", "unseen").toDF("category")
    assertEquals(vectors(keep.transform(test), "category_idx_vec"), Seq(Seq(1.0,0.0), Seq(0.0,1.0), Seq(0.0,0.0)))
    assert(keep.transform(test).schema("category_idx_vec").metadata.contains("ml_attr"))
  }

  test("numeric RFormula preserves feature ordering metadata and works without a response at transform time") {
    val session = spark; import session.implicits._
    val train = Seq((1.0,2.0,7.0),(3.0,5.0,16.0)).toDF("x","z","y")
    val model = FeaturePreparation.design(train,"y ~ x + z - 1","features","label",false)
    val encoded = model.transform(train)
    assertEquals(vectors(encoded,"features"), Seq(Seq(1.0,2.0),Seq(3.0,5.0)))
    assertEquals(encoded.select("label").limit(10).collect().map(_.getDouble(0)).toSeq,Seq(7.0,16.0))
    assertEquals(encoded.schema("features").metadata.getMetadata("ml_attr").getLong("num_attrs"),2L)
    val test = Seq((9.0,4.0)).toDF("x","z")
    assertEquals(vectors(model.transform(test),"features"),Seq(Seq(9.0,4.0)))
    intercept[Exception](FeaturePreparation.design(train,"y ~ >","features","label",false))
  }

  test("categorical RFormula does not learn a test-only category") {
    val session = spark; import session.implicits._
    val train = Seq((1.0,"a",5.0),(2.0,"b",7.0),(3.0,"a",9.0)).toDF("x","category","y")
    val model = FeaturePreparation.design(train,"y ~ x + category","features","label",true)
    val test = Seq((4.0,"never_in_training")).toDF("x","category")
    val before = vectors(model.transform(train),"features")
    val after = vectors(model.transform(test),"features")
    assertEquals(after.size,1)
    assertEquals(after.head.size,before.head.size)
    assertEquals(after.head.head,4.0)
    assertEquals(vectors(model.transform(train),"features"),before)
  }

  test("empty training collisions missing columns and dotted names are explicit") {
    val session = spark; import session.implicits._
    val train = Seq("a","b").toDF("category")
    val empty = train.limit(0)
    assert(intercept[IllegalArgumentException](FeaturePreparation.index(empty,Array("category"),false,false,false)).getMessage.contains("empty"))
    intercept[IllegalArgumentException](FeaturePreparation.index(train.withColumn("category_idx",col("category")),Array("category"),false,false,false))
    intercept[IllegalArgumentException](FeaturePreparation.index(train,Array("missing"),false,false,false))
    val dotted = train.withColumnRenamed("category","category.name")
    val model = FeaturePreparation.index(dotted,Array("category.name"),false,false,false)
    assertEquals(model.transform(dotted).select(col("`category.name_idx`")).limit(10).collect().map(_.getDouble(0)).toSeq,Seq(0.0,1.0))
  }
}
