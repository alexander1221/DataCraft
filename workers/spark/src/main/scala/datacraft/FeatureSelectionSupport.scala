package datacraft

import org.apache.spark.ml.{Pipeline, Transformer}
import org.apache.spark.ml.attribute.{Attribute, AttributeGroup, NominalAttribute, NumericAttribute}
import org.apache.spark.ml.feature.{QuantileDiscretizer, StringIndexer, StringIndexerModel, VectorAssembler, VectorSizeHint, VectorSlicer}
import org.apache.spark.ml.linalg.{SQLDataTypes, Vector}
import org.apache.spark.ml.param.ParamMap
import org.apache.spark.ml.regression.RandomForestRegressionModel
import org.apache.spark.ml.classification.RandomForestClassificationModel
import org.apache.spark.ml.stat.ChiSquareTest
import org.apache.spark.ml.util.Identifiable
import org.apache.spark.sql.{DataFrame, Dataset, Row, SparkSession}
import org.apache.spark.sql.functions.{abs, col, isnan, lit}
import org.apache.spark.sql.types.{BooleanType, DoubleType, LongType, NumericType, StringType, StructType}
import ujson.{Obj, Value}

/** Train-only selection and bounded reports. Contract: docs/FEATURE_SELECTION.md. */
private[datacraft] object FeatureSelectionSupport {
  val MaxFeatures = 1000
  final case class Result(model: Option[Transformer], report: DataFrame)
  private def exact(name: String) = col(FeaturePreparation.quoted(name))
  private def integer(config: Value, name: String, min: Int, max: Int): Int = {
    val v=config(name).num
    require(v.isFinite && v==math.floor(v) && v>=min && v<=max,s"$name must be an integer in [$min,$max]")
    v.toInt
  }
  private def list(config: Value, name: String): Array[String] = {
    val xs=config(name).arr.map(_.str).toArray
    require(xs.length<=MaxFeatures && xs.forall(_.trim.nonEmpty) && xs.distinct.length==xs.length,s"Invalid $name list")
    xs
  }
  private def finiteColumns(df: DataFrame, names: Seq[String]): Unit = {
    val bad=names.map { name =>
      val c=exact(name)
      val missing=c.isNull
      if (df.schema(name).dataType.isInstanceOf[NumericType]) missing || isnan(c.cast("double")) ||
        (abs(c.cast("double")) === lit(Double.PositiveInfinity)) else missing
    }
    bad.reduceOption(_ || _).foreach(condition => require(df.filter(condition).limit(1).count()==0,
      "Selection input contains null/NaN/Infinity values"))
  }
  private def schemaCheck(schema: StructType, columns: Seq[String], categories: Set[String]): Unit = {
    columns.foreach { name =>
      require(schema.fieldNames.contains(name),s"Unknown column: $name")
      val dataType=schema(name).dataType
      require(dataType.isInstanceOf[NumericType] || (categories(name) && dataType==StringType),
        s"Selection column has incompatible type: $name")
    }
  }
  private def cardinality(df: DataFrame, name: String, minimum: Long): Unit = {
    val count=df.select(exact(name)).distinct().limit(1001).count()
    require(count>=minimum && count<=1000,s"Column $name requires $minimum..1000 categories; prepare a categorical target explicitly")
  }
  private def top(config: Value, available: Int): Int = {
    val n=integer(config,"num_top_features",1,MaxFeatures)
    require(n<=available,s"num_top_features=$n exceeds $available usable features")
    n
  }
  private def output(config: Value, df: DataFrame): String = {
    val name=config("output_column").str
    require(name.trim.nonEmpty && !df.columns.contains(name),s"Output column already exists or is empty: $name")
    name
  }

  def fit(spark: SparkSession, df: DataFrame, config: Value): Result = {
    if (config("kind").str=="random_forest_pipeline") return forest(spark,df,config)
    val columns=list(config,"columns")
    require(columns.nonEmpty,"Selection needs candidate columns")
    val cats=list(config,"categorical_columns").toSet
    require(cats.subsetOf(columns.toSet),"Categorical columns must be a subset of candidates")
    val label=config("label_column").str
    require(label.trim.nonEmpty && !columns.contains(label),"Target must differ from candidates")
    schemaCheck(df.schema,columns.toSeq,cats)
    schemaCheck(df.schema,Seq(label),Set(label))
    finiteColumns(df,columns.toSeq :+ label)
    require(df.limit(1).count()>0,"Training table is empty")
    cardinality(df,label,2)
    cats.toSeq.sorted.foreach(c=>cardinality(df,c,1))
    val buckets=integer(config,"num_buckets",2,1000)
    val error=config("relative_error").num
    require(error.isFinite && error>=0 && error<=1,"Quantile relative error must be in [0,1]")
    val used=scala.collection.mutable.Set.from(df.columns)
    val pipeline=config("kind").str=="feature_selection_pipeline"
    if (pipeline) used+=output(config,df)
    def temporary(): String = {
      var i=used.size
      while (used(s"__datacraft_selection_$i")) i+=1
      val name=s"__datacraft_selection_$i"; used+=name; name
    }
    val encoded=columns.map(_=>temporary())
    val categorical=columns.filter(cats)
    val continuous=columns.filterNot(cats)
    val indexedOutputs=categorical.map(c=>encoded(columns.indexOf(c)))
    val indexer=if (categorical.nonEmpty) Some(new StringIndexer()
      .setInputCols(categorical.map(FeaturePreparation.quoted)).setOutputCols(indexedOutputs)
      .setStringOrderType("alphabetAsc").setHandleInvalid(
        if (config("kind").str=="feature_selection_pipeline" && config("keep_unknown").bool) "keep" else "error").fit(df)) else None
    var prepared=indexer.map(_.transform(df)).getOrElse(df)
    if (continuous.nonEmpty) {
      val bucketizer=new QuantileDiscretizer().setInputCols(continuous.map(FeaturePreparation.quoted))
        .setOutputCols(continuous.map(c=>encoded(columns.indexOf(c)))).setNumBuckets(buckets)
        .setRelativeError(error).setHandleInvalid("error").fit(df)
      prepared=bucketizer.transform(prepared)
    }
    val indexedLabel=temporary()
    val labelModel=new StringIndexer().setInputCol(FeaturePreparation.quoted(label)).setOutputCol(indexedLabel)
      .setStringOrderType("alphabetAsc").setHandleInvalid("error").fit(df)
    prepared=labelModel.transform(prepared)
    val vectorName=temporary()
    prepared=new VectorAssembler().setInputCols(encoded).setOutputCol(vectorName).setHandleInvalid("error").transform(prepared)
    // Exactly one row containing at most 1000 entries per vector.
    val stats=ChiSquareTest.test(prepared,vectorName,indexedLabel).head()
    val p=stats.getAs[Vector]("pValues").toArray
    val statistic=stats.getAs[Vector]("statistics").toArray
    val degrees=stats.getAs[scala.collection.Seq[Int]]("degreesOfFreedom")
    require(p.length==columns.length && statistic.length==columns.length && degrees.length==columns.length &&
      columns.indices.forall(i=>statistic(i).isFinite && statistic(i)>=0 && degrees(i)>=0 &&
        (degrees(i)==0 || (p(i).isFinite && p(i)>=0 && p(i)<=1))),"Invalid chi-square result")
    val usable=columns.indices.filter(i=>degrees(i)>0 && p(i).isFinite && p(i)>=0 && p(i)<=1)
    val selected=if (pipeline) usable.sortBy(i=>(p(i),i)).take(top(config,usable.length)).sorted.toArray else Array.emptyIntArray
    val chosen=selected.toSet
    val reportSchema=new StructType().add("feature",StringType,false).add("statistic",DoubleType,false)
      .add("degrees_of_freedom",LongType,false).add("p_value",DoubleType,true)
    val fullSchema=if (pipeline) reportSchema.add("selected",BooleanType,false) else reportSchema
    val rows=columns.indices.map { i =>
      val probability:java.lang.Double=if (degrees(i)>0 && p(i).isFinite) Double.box(p(i)) else null
      val values=Seq(columns(i),statistic(i),degrees(i).toLong,probability)
      Row.fromSeq(if (pipeline) values :+ chosen(i) else values)
    }
    import scala.jdk.CollectionConverters._
    val report=spark.createDataFrame(rows.asJava,fullSchema)
    val model=if (pipeline) {
      val name=output(config,df)
      val selectedColumns=selected.map(columns)
      val sources=selected.map(i=>if (cats(columns(i))) encoded(i) else columns(i))
      Some(new PreparedSelectionModel(Identifiable.randomUID("feature_selection"),columns,cats,indexer,
        indexedOutputs,selectedColumns,sources,name))
    } else None
    Result(model,report)
  }

  private def forest(spark: SparkSession, df: DataFrame, config: Value): Result = {
    val features=config("features_column").str
    FeaturePreparation.requireColumns(df,Seq(features))
    require(df.schema(features).dataType==SQLDataTypes.VectorType,"Features must be a Spark ML vector")
    val first=df.select(exact(features)).limit(1).collect()
    require(first.nonEmpty && !first(0).isNullAt(0),"Training table is empty or vector is null")
    val dimension=first(0).getAs[Vector](0).size
    require(dimension>=1 && dimension<=MaxFeatures,"Vector dimension must be in 1..1000")
    val count=top(config,dimension)
    val name=output(config,df)
    val task=config("task").str
    require(task=="classification" || task=="regression","Invalid forest selection task")
    val copied=Obj.from(config.obj.toSeq.filterNot {case (key,_) => Set("output_column","num_top_features","task")(key)})
    copied("kind")=if (task=="classification") "random_forest_classifier" else "random_forest_regressor"
    val importance=if (task=="classification") {
      copied("impurity")="gini"
      ClassificationSupport.fit(df,copied).asInstanceOf[RandomForestClassificationModel].featureImportances.toArray
    } else RegressionSupport.fit(df,copied).asInstanceOf[RandomForestRegressionModel].featureImportances.toArray
    require(importance.length==dimension && importance.forall(v=>v.isFinite && v>=0),"Invalid forest importance")
    val selected=importance.indices.sortBy(i=>(-importance(i),i)).take(count).sorted.toArray
    val chosen=selected.toSet
    val group=AttributeGroup.fromStructField(df.schema(features))
    val rows=importance.indices.map { i => Row(i.toLong,
      group.attributes.flatMap(_.lift(i)).flatMap(_.name).getOrElse(s"feature[$i]"),importance(i),chosen(i)) }
    val schema=new StructType().add("feature_index",LongType,false).add("feature",StringType,false)
      .add("importance",DoubleType,false).add("selected",BooleanType,false)
    import scala.jdk.CollectionConverters._
    val report=spark.createDataFrame(rows.asJava,schema)
    val hint=new VectorSizeHint().setInputCol(features).setSize(dimension).setHandleInvalid("error")
    val slicer=new VectorSlicer().setInputCol(features).setOutputCol(name).setIndices(selected)
    val model=new Pipeline().setStages(Array(hint,slicer)).fit(df)
    Result(Some(model),report)
  }

  private final class PreparedSelectionModel(override val uid: String, columns: Array[String], categories: Set[String],
      indexer: Option[StringIndexerModel], temporary: Array[String], selectedNames: Array[String], sources: Array[String], target: String) extends Transformer {
    private def assembler = new VectorAssembler().setInputCols(sources.map(FeaturePreparation.quoted)).setOutputCol(target).setHandleInvalid("error")
    override def transform(dataset: Dataset[_]): DataFrame = {
      val resultSchema=transformSchema(dataset.schema)
      val df=dataset.toDF()
      finiteColumns(df,selectedNames.filterNot(categories).toSeq)
      val indexed=indexer.map(_.transform(df)).getOrElse(df)
      assembler.transform(indexed).withColumn(target,exact(target).as(target,resultSchema(target).metadata))
        .drop(temporary.toIndexedSeq:_*)
    }
    override def transformSchema(schema: StructType): StructType = {
      schemaCheck(schema,columns.toSeq,categories)
      val indexed=indexer.map(_.transformSchema(schema)).getOrElse(schema)
      val result=assembler.transformSchema(indexed)
      // VectorAssembler.transformSchema does not populate attributes. Reconstruct
      // them from the fitted dictionary instead of losing names/category metadata.
      val categorical=columns.filter(categories)
      val attributes:Array[Attribute]=selectedNames.map { name =>
        if (categories(name)) {
          val fitted=indexer.getOrElse(throw new IllegalArgumentException("Missing fitted category indexer"))
          val known=fitted.labelsArray(categorical.indexOf(name))
          val values=if(fitted.getHandleInvalid=="keep")known:+"__unknown" else known
          NominalAttribute.defaultAttr.withName(name).withValues(values)
        } else NumericAttribute.defaultAttr.withName(name)
      }
      val metadata=new AttributeGroup(target,attributes).toMetadata()
      StructType(result.fields.filterNot(f=>temporary.contains(f.name)).map(f=>if (f.name==target) f.copy(metadata=metadata) else f))
    }
    override def copy(extra: ParamMap): Transformer = copyValues(new PreparedSelectionModel(uid,columns.clone(),categories,
      indexer.map(_.copy(ParamMap.empty)),temporary.clone(),selectedNames.clone(),sources.clone(),target),extra)
  }
}
