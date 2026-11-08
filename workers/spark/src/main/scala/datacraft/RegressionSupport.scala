package datacraft

import org.apache.spark.ml.Transformer
import org.apache.spark.ml.linalg.SQLDataTypes
import org.apache.spark.ml.regression.{DecisionTreeRegressionModel, DecisionTreeRegressor,
  GeneralizedLinearRegression, GeneralizedLinearRegressionModel, LinearRegressionModel,
  RandomForestRegressionModel, RandomForestRegressor}
import org.apache.spark.sql.DataFrame
import org.apache.spark.sql.functions.{abs, avg, col, count, isnan, lit, sum, var_pop, when}
import org.apache.spark.sql.types.NumericType
import ujson.Value

/** Training and aggregate evaluation. Contract and independent oracles: docs/REGRESSION.md. */
private[datacraft] object RegressionSupport {
  private def exact(name: String) = col(FeaturePreparation.quoted(name))
  private def integer(config: Value, name: String, min: Long, max: Long): Long = {
    val value = config(name).num
    require(value.isFinite && value == math.floor(value) && value >= min && value <= max,
      s"$name must be an integer in [$min,$max]")
    value.toLong
  }
  private def numeric(df: DataFrame, names: Seq[String]): Unit = {
    FeaturePreparation.requireColumns(df, names)
    names.foreach(name => require(df.schema(name).dataType.isInstanceOf[NumericType],
      s"Column must be numeric: $name"))
  }
  private def finite(value: Double, description: String): Double = {
    require(value.isFinite, s"Non-finite $description; check values and numeric scale")
    value
  }
  def isRegression(model: Transformer): Boolean = model match {
    case _: LinearRegressionModel | _: GeneralizedLinearRegressionModel |
         _: DecisionTreeRegressionModel | _: RandomForestRegressionModel => true
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
    require(!df.columns.contains("prediction"), "Reserved column already exists: prediction")
    require(df.limit(1).count() > 0, "Training table is empty")
    config("kind").str match {
      case "generalized_linear_regression" =>
        val family = config("family").str
        val link = config("link").str
        val allowed = Map("gaussian" -> Seq("identity", "log", "inverse"),
          "binomial" -> Seq("logit", "probit", "cloglog"),
          "poisson" -> Seq("log", "identity", "sqrt"), "gamma" -> Seq("inverse", "identity", "log"))
        require(allowed.contains(family), s"Unsupported GLM family: $family")
        require(link == "default" || allowed(family).contains(link), s"Invalid GLM family/link: $family/$link")
        val response = exact(label).cast("double")
        val invalid = response.isNull || isnan(response) || (abs(response) === lit(Double.PositiveInfinity)) ||
          (family match {
            case "binomial" => response < 0 || response > 1
            case "poisson" => response < 0
            case "gamma" => response <= 0
            case _ => lit(false)
          })
        require(df.filter(invalid).limit(1).count() == 0, s"Invalid response values for GLM family $family")
        val reg = finite(config("reg_param").num, "regularization")
        require(reg >= 0, "Regularization must be nonnegative")
        val estimator = new GeneralizedLinearRegression().setFamily(family)
          .setLabelCol(label).setFeaturesCol(features).setFitIntercept(config("fit_intercept").bool)
          .setMaxIter(integer(config, "max_iter", 1, Int.MaxValue).toInt).setRegParam(reg)
        if (link != "default") estimator.setLink(link)
        estimator.fit(df)
      case kind @ ("decision_tree_regressor" | "random_forest_regressor") =>
        val depth = integer(config, "max_depth", 0, 30).toInt
        val bins = integer(config, "max_bins", 2, Int.MaxValue).toInt
        val minimum = integer(config, "min_instances_per_node", 1, Int.MaxValue).toInt
        val seed = integer(config, "seed", 0, 4294967295L)
        if (kind == "decision_tree_regressor") {
          new DecisionTreeRegressor().setLabelCol(label).setFeaturesCol(features)
            .setMaxDepth(depth).setMaxBins(bins).setMinInstancesPerNode(minimum).setSeed(seed).fit(df)
        } else {
          val strategy = config("feature_subset_strategy").str
          require(Set("auto", "all", "sqrt", "log2", "onethird").contains(strategy), "Invalid feature subset strategy")
          new RandomForestRegressor().setLabelCol(label).setFeaturesCol(features)
            .setMaxDepth(depth).setMaxBins(bins).setMinInstancesPerNode(minimum).setSeed(seed)
            .setNumTrees(integer(config, "num_trees", 1, Int.MaxValue).toInt)
            .setBootstrap(config("bootstrap").bool).setFeatureSubsetStrategy(strategy).setSubsamplingRate(1.0).fit(df)
        }
      case other => throw new IllegalArgumentException(s"Unsupported regression: $other")
    }
  }

  def metrics(df: DataFrame, label: String, prediction: String): Seq[(String, Option[Double])] = {
    require(label.trim.nonEmpty && prediction.trim.nonEmpty && label != prediction,
      "Label and prediction columns must be nonempty and different")
    numeric(df, Seq(label, prediction))
    val y = exact(label).cast("double")
    val p = exact(prediction).cast("double")
    val bad = y.isNull || p.isNull || isnan(y) || isnan(p) ||
      (abs(y) === lit(Double.PositiveInfinity)) || (abs(p) === lit(Double.PositiveInfinity))
    val difference = p - y
    // One aggregate row reaches the driver, independent of dataset size.
    val row = df.agg(count(lit(1)), sum(when(bad, 1L).otherwise(0L)),
      sum(difference * difference), avg(abs(difference)), var_pop(y)).head()
    val n = row.getLong(0)
    require(n > 0, "Evaluation table is empty")
    require(row.getLong(1) == 0, "Evaluation contains null/NaN/Infinity labels or predictions")
    val mse = finite(row.getDouble(2), "sum of squared errors") / n.toDouble
    val mae = finite(row.getDouble(3), "MAE")
    val variance = finite(row.getDouble(4), "label variance")
    require(variance >= 0, "Negative label variance")
    val r2 = if (variance == 0) None else Some(finite(1 - mse / variance, "R2"))
    Seq("rmse" -> Some(math.sqrt(mse)), "mse" -> Some(mse), "mae" -> Some(mae), "r2" -> r2)
  }
}
