package datacraft

import org.apache.spark.ml.Transformer
import org.apache.spark.ml.classification.{DecisionTreeClassificationModel, DecisionTreeClassifier,
  LinearSVC, LinearSVCModel, LogisticRegression, LogisticRegressionModel,
  RandomForestClassificationModel, RandomForestClassifier}
import org.apache.spark.ml.linalg.SQLDataTypes
import org.apache.spark.sql.{Column, DataFrame}
import org.apache.spark.sql.functions.{abs, col, floor, isnan, lit}
import org.apache.spark.sql.types.NumericType
import ujson.Value

/** Train-only classifiers and bounded aggregate evaluation. See docs/CLASSIFICATION.md. */
private[datacraft] object ClassificationSupport {
  private val MaxClasses = 1000
  private val MaxCells = 1000
  private def exact(name: String) = col(FeaturePreparation.quoted(name))
  private def numeric(df: DataFrame, names: Seq[String]): Unit = {
    FeaturePreparation.requireColumns(df, names)
    names.foreach(name => require(df.schema(name).dataType.isInstanceOf[NumericType],
      s"Column must be numeric: $name"))
  }
  private def integer(config: Value, name: String, min: Long, max: Long): Long = {
    val value = config(name).num
    require(value.isFinite && value == math.floor(value) && value >= min && value <= max,
      s"$name must be an integer in [$min,$max]")
    value.toLong
  }
  private def nonnegative(config: Value, name: String): Double = {
    val value = config(name).num
    require(value.isFinite && value >= 0, s"$name must be finite and nonnegative")
    value
  }
  private def invalid(c: Column): Column = c.isNull || isnan(c) ||
    (abs(c) === lit(Double.PositiveInfinity)) || (c < 0) || (c > Int.MaxValue) || (c =!= floor(c))

  def isClassifier(model: Transformer): Boolean = model match {
    case _: LogisticRegressionModel | _: LinearSVCModel |
         _: DecisionTreeClassificationModel | _: RandomForestClassificationModel => true
    case _ => false
  }

  def fit(df: DataFrame, config: Value): Transformer = {
    val label = config("label_column").str
    val features = config("features_column").str
    require(label.trim.nonEmpty && features.trim.nonEmpty && label != features,
      "Label and features columns must be nonempty and different")
    numeric(df, Seq(label))
    FeaturePreparation.requireColumns(df, Seq(features))
    require(df.schema(features).dataType == SQLDataTypes.VectorType, "Features must be a Spark ML vector")
    Seq("prediction", "probability", "rawPrediction").foreach(name =>
      require(!df.columns.contains(name), s"Reserved column already exists: $name"))
    val response = exact(label).cast("double")
    require(df.filter(invalid(response)).limit(1).count() == 0,
      "Class labels must be finite nonnegative int32 IDs without nulls")
    // Bounded collection even for malformed high-cardinality labels.
    val labels = df.select(response).distinct().limit(MaxClasses + 1).collect().map(_.getDouble(0)).sorted
    require(labels.nonEmpty, "Training table is empty")
    require(labels.length >= 2 && labels.length <= MaxClasses &&
      labels.zipWithIndex.forall { case (value, index) => value == index.toDouble },
      "Training labels must contain contiguous classes 0..K-1 with 2 <= K <= 1000")
    config("kind").str match {
      case kind @ ("logistic_regression" | "linear_svc") =>
        require(labels.length == 2, "Logistic Regression and Linear SVM require binary classes 0/1")
        val iterations = integer(config, "max_iter", 1, Int.MaxValue).toInt
        val reg = nonnegative(config, "reg_param")
        val threshold = config("threshold").num
        require(threshold.isFinite, "Threshold must be finite")
        val intercept = config("fit_intercept").bool
        if (kind == "logistic_regression") {
          val elastic = nonnegative(config, "elastic_net_param")
          require(elastic <= 1 && threshold >= 0 && threshold <= 1,
            "Elastic net and probability threshold must be in [0,1]")
          new LogisticRegression().setFamily("binomial").setLabelCol(label).setFeaturesCol(features)
            .setFitIntercept(intercept).setMaxIter(iterations).setRegParam(reg)
            .setElasticNetParam(elastic).setThreshold(threshold).fit(df)
        } else {
          new LinearSVC().setLabelCol(label).setFeaturesCol(features).setFitIntercept(intercept)
            .setMaxIter(iterations).setRegParam(reg).setThreshold(threshold).fit(df)
        }
      case kind @ ("decision_tree_classifier" | "random_forest_classifier") =>
        val depth = integer(config, "max_depth", 0, 30).toInt
        val bins = integer(config, "max_bins", 2, Int.MaxValue).toInt
        val minimum = integer(config, "min_instances_per_node", 1, Int.MaxValue).toInt
        val seed = integer(config, "seed", 0, 4294967295L)
        val impurity = config("impurity").str
        require(Set("gini", "entropy").contains(impurity), "Impurity must be gini or entropy")
        if (kind == "decision_tree_classifier") {
          new DecisionTreeClassifier().setLabelCol(label).setFeaturesCol(features).setSeed(seed)
            .setMaxDepth(depth).setMaxBins(bins).setMinInstancesPerNode(minimum).setImpurity(impurity).fit(df)
        } else {
          val strategy = config("feature_subset_strategy").str
          require(Set("auto", "all", "sqrt", "log2", "onethird").contains(strategy), "Invalid feature subset strategy")
          new RandomForestClassifier().setLabelCol(label).setFeaturesCol(features).setSeed(seed)
            .setMaxDepth(depth).setMaxBins(bins).setMinInstancesPerNode(minimum).setImpurity(impurity)
            .setNumTrees(integer(config, "num_trees", 1, Int.MaxValue).toInt).setBootstrap(config("bootstrap").bool)
            .setFeatureSubsetStrategy(strategy).setSubsamplingRate(1.0).fit(df)
        }
      case other => throw new IllegalArgumentException(s"Unsupported classifier: $other")
    }
  }

  def metrics(df: DataFrame, label: String, prediction: String): Seq[(String, Double)] = {
    require(label.trim.nonEmpty && prediction.trim.nonEmpty && label != prediction,
      "Label and prediction columns must be nonempty and different")
    numeric(df, Seq(label, prediction))
    val selected = df.select(exact(label).cast("double").as("label"), exact(prediction).cast("double").as("prediction"))
    require(selected.filter(invalid(col("label")) || invalid(col("prediction"))).limit(1).count() == 0,
      "Evaluation classes must be finite nonnegative int32 IDs without nulls")
    val cells = selected.groupBy("label", "prediction").count().limit(MaxCells + 1).collect()
    require(cells.nonEmpty, "Evaluation table is empty")
    require(cells.length <= MaxCells, "Evaluation exceeds 1000 occupied confusion-matrix cells")
    val support = scala.collection.mutable.Map.empty[Double, Double].withDefaultValue(0.0)
    val predicted = scala.collection.mutable.Map.empty[Double, Double].withDefaultValue(0.0)
    val truePositive = scala.collection.mutable.Map.empty[Double, Double].withDefaultValue(0.0)
    cells.foreach { row =>
      val y = row.getDouble(0); val p = row.getDouble(1); val count = row.getLong(2).toDouble
      require(count > 0, "Invalid confusion count")
      support(y) += count
      predicted(p) += count
      if (y == p) truePositive(y) += count
    }
    // A deterministic local summation order for the bounded set of label IDs.
    val classes = support.keys.toSeq.sorted
    val total = classes.map(support).sum
    var precision = 0.0; var f1 = 0.0
    classes.foreach { y =>
      val tp = truePositive(y)
      val p = if (predicted(y) == 0) 0.0 else tp / predicted(y)
      val f = 2 * tp / (support(y) + predicted(y))
      // Divide once after summation: perfect classification remains exactly 1
      // for small exact counts even when the number of classes is large.
      precision += support(y) * p; f1 += support(y) * f
    }
    val accuracy = classes.map(truePositive).sum / total
    Seq("accuracy" -> accuracy, "weighted_precision" -> (precision / total),
      "weighted_recall" -> accuracy, "weighted_f1" -> (f1 / total))
  }
}
