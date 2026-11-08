package datacraft

import java.nio.charset.StandardCharsets
import java.nio.file.{Files, Path, Paths}
import java.util.UUID
import org.apache.spark.ml.feature.VectorAssembler
import org.apache.spark.ml.Transformer
import org.apache.spark.ml.linalg.SQLDataTypes
import org.apache.spark.ml.regression.LinearRegression
import org.apache.spark.sql.{DataFrame, Row, SparkSession}
import org.apache.spark.sql.functions.col
import org.apache.spark.sql.types.{DoubleType, NumericType, StringType, StructType}
import org.apache.spark.storage.StorageLevel
import scala.collection.mutable
import scala.util.control.NonFatal
import ujson.{Arr, Null, Num, Obj, Str, Value}

/** Local-only worker. stdout is exclusively newline-delimited protocol JSON. */
object Worker {
  val ProtocolVersion = 1
  val MaxPreviewRows = 1000
  val MaxResponseBytes = 8 * 1024 * 1024

  /** Runtime-only state: never serialized into project files. */
  final class Cache {
    val tables = mutable.Map.empty[(String, String), DataFrame]
    val models = mutable.Map.empty[String, Transformer]
    val tokens = mutable.Map.empty[String, String]
    def remove(node: String): Unit = {
      tables.keys.filter(_._1 == node).toVector.foreach { key => tables.remove(key).foreach(_.unpersist(false)) }
      models.remove(node)
      tokens.remove(node)
    }
  }

  def main(args: Array[String]): Unit = {
    val protocol = System.out
    // Spark and third-party libraries may print to stdout during initialization.
    System.setOut(System.err)
    val spark = SparkSession.builder().appName("DataCraft")
      .master("local[2]").config("spark.ui.enabled", "false")
      .config("spark.driver.bindAddress", "127.0.0.1")
      .config("spark.driver.host", "127.0.0.1")
      .config("spark.sql.shuffle.partitions", "2").getOrCreate()
    spark.sparkContext.setLogLevel("WARN")
    val source = scala.io.Source.fromInputStream(System.in, "UTF-8")
    val cache = new Cache
    try {
      source.getLines().foreach { line =>
        val response = try execute(spark, ujson.read(line), cache, frame => {
          protocol.println(ujson.write(frame)); protocol.flush()
        }) catch {
          case NonFatal(error) => failure(Null, None, error)
        }
        protocol.println(ujson.write(response))
        protocol.flush()
      }
    } finally {
      source.close()
      spark.stop()
    }
  }

  private def failure(id: Value, node: Option[String], error: Throwable): Value = Obj(
    "protocol_version" -> ProtocolVersion,
    "request_id" -> id,
    "ok" -> false,
    "node" -> node.map(Str(_)).getOrElse(Null),
    "message" -> Option(error.getMessage).getOrElse(error.getClass.getSimpleName).take(4096)
  )

  private def localPath(value: String): Path = {
    require(!value.contains("://"), "Only local filesystem paths are supported")
    val path = Paths.get(value)
    require(path.isAbsolute, "Worker requires resolved absolute source paths")
    require(Files.exists(path), s"Source does not exist: $path")
    path
  }

  private def names(config: Value): Array[String] = {
    val columns = config("columns").arr.map(_.str).toArray
    require(columns.nonEmpty && columns.forall(_.trim.nonEmpty) && columns.distinct.length == columns.length,
      "Column selection must be nonempty and unique")
    columns
  }

  private def exactColumn(name: String) = col("`" + name.replace("`", "``") + "`")

  private def requireColumns(df: DataFrame, columns: Seq[String]): Unit = {
    columns.foreach(name => require(df.columns.contains(name), s"Unknown column: $name"))
  }

  private def finite(value: Double): Value = if (value.isNaN || value.isInfinity) Null else Num(value)

  private def preview(name: String, df: DataFrame, rows: Int): Value = {
    require(rows >= 1 && rows <= MaxPreviewRows, "Preview rows must be between 1 and 1000")
    // Both row and total response byte limits apply. Wide values can still be expensive in Spark.
    val bounded = df.limit(rows)
    val fields = df.schema.fieldNames
    val values = bounded.toJSON.collect().map { json =>
      val row = ujson.read(json)
      // Spark's JSON generator omits null fields by default; preserve table cells.
      fields.foreach(name => if (!row.obj.contains(name)) row(name) = Null)
      row
    }
    Obj("name" -> name, "schema" -> ujson.read(df.schema.json), "rows" -> Arr.from(values))
  }

  private def report(spark: SparkSession, name: String, columns: Seq[String], rows: Seq[Seq[Value]]): Value = {
    import spark.implicits._
    // Reports are small local tables; derive their Spark schema using a bounded JSON dataset.
    val objects = rows.map(values => Obj.from(columns.zip(values)))
    val df = spark.read.json(spark.createDataset(objects.map(ujson.write(_))))
      .select(columns.map(exactColumn): _*)
    preview(name, df, MaxPreviewRows)
  }

  def execute(spark: SparkSession, request: Value, cache: Cache = new Cache,
      emit: Value => Unit = _ => ()): Value = {
    val id = request.obj.getOrElse("request_id", Null)
    var activeNode: Option[String] = None
    try {
      UUID.fromString(id.str)
      require(request("protocol_version").num == ProtocolVersion, "Unsupported protocol version")
      val cached = request("operation").str == "run_nodes"
      require(cached || request("operation").str == "run", "Unsupported operation")
      val project = request("project")
      require(project("format_version").num == 1, "Unsupported project format")
      val nodes = project("nodes").arr.map(n => n("id").str -> n("config")).toMap
      require(nodes.size == project("nodes").arr.size, "Duplicate node IDs")
      val edges = project("edges").arr
      val order = request("execution_order").arr.map(_.str)
      val output = request("output_node").str
      require(order.nonEmpty && (cached || order.last == output) && order.distinct.size == order.size,
        "Invalid execution order")
      val tables = if (cached) cache.tables else mutable.Map.empty[(String, String), DataFrame]
      val models = if (cached) cache.models else mutable.Map.empty[String, Transformer]
      val tokens = if (cached) request("cache_tokens").obj.view.mapValues(_.str).toMap else Map.empty[String, String]
      if (cached) cache.tokens.keys.toVector.foreach { node =>
        if (!tokens.get(node).contains(cache.tokens(node)) || order.contains(node)) cache.remove(node)
      }
      def progress(node: String, phase: String): Unit = {
        if (request.obj.get("progress").exists(_.bool)) emit(Obj(
          "protocol_version" -> ProtocolVersion, "request_id" -> id,
          "event" -> "node", "node" -> node, "phase" -> phase))
      }
      var results = Vector.empty[Value]
      var allResults = Vector.empty[Value]

      def upstreamEdge(node: String, port: String): Value = {
        val matches = edges.filter(e => e("to").str == node && e("to_port").str == port)
        require(matches.size == 1, s"Expected exactly one input: $node:$port")
        matches.head
      }
      def input(node: String): DataFrame = {
        val edge = upstreamEdge(node, "table")
        tables.getOrElse((edge("from").str, edge("from_port").str),
          throw new IllegalArgumentException(s"Missing table dependency for $node"))
      }
      def table(node: String, df: DataFrame): Unit = { tables((node, "table")) = df }
      def fitted(node: String, model: Transformer, training: DataFrame): Unit = {
        models(node) = model
        table(node, model.transform(training))
      }

      spark.sparkContext.setJobGroup(id.str, "DataCraft execution", interruptOnCancel = true)
      order.foreach { node =>
        activeNode = Some(node)
        progress(node, "running")
        if (cached) results = Vector.empty
        val config = nodes.getOrElse(node, throw new IllegalArgumentException(s"Unknown node $node"))
        config("kind").str match {
          case "csv_source" =>
            val path = localPath(config("path").str)
            table(node, spark.read.option("header", config("header").bool)
              .option("inferSchema", config("infer_schema").bool)
              .option("mode", "FAILFAST").csv(path.toString))
          case "parquet_source" =>
            table(node, spark.read.parquet(localPath(config("path").str).toString))
          case "json_source" =>
            table(node, spark.read.option("mode", "FAILFAST").json(localPath(config("path").str).toString))
          case "train_test_split" =>
            val seed = config("seed").num
            require(seed.isFinite && seed == math.floor(seed), "Seed must be an integer")
            val keys = config("key_columns").arr.map(_.str).toSeq
            val (train, test) = FeaturePreparation.split(input(node), keys, config("train_fraction").num, seed.toLong)
            tables((node, "train")) = train
            tables((node, "test")) = test
          case "index_categoricals" | "reference_code_categoricals" =>
            val df = input(node)
            val oneHot = config("kind").str == "reference_code_categoricals"
            val model = FeaturePreparation.index(df, names(config), config("keep_unknown").bool,
              oneHot, if (oneHot) config("drop_last").bool else false)
            fitted(node, model, df)
          case "make_design_matrix" =>
            val df = input(node)
            fitted(node, FeaturePreparation.design(df, config("formula").str, config("features_column").str,
              config("label_column").str, config("keep_unknown").bool), df)
          case "filter" => table(node, input(node).filter(config("expression").str))
          case "feature_selection" | "feature_selection_pipeline" | "random_forest_pipeline" =>
            val training=input(node)
            val selection=FeatureSelectionSupport.fit(spark,training,config)
            selection.model.foreach(model=>fitted(node,model,training))
            if (cached || node==output) results=Vector(preview("Feature selection",selection.report,MaxPreviewRows))
          case "select_columns" =>
            val df = input(node)
            val columns = names(config)
            requireColumns(df, columns.toSeq)
            table(node, df.select(columns.toIndexedSeq.map(exactColumn): _*))
          case "limit" =>
            val rows = config("rows").num.toInt
            require(rows > 0, "Limit must be positive")
            table(node, input(node).limit(rows))
          case "assemble_features" =>
            val df = input(node)
            val columns = names(config)
            requireColumns(df, columns.toSeq)
            columns.foreach(c => require(df.schema(c).dataType.isInstanceOf[NumericType] || df.schema(c).dataType == SQLDataTypes.VectorType,
              s"Feature must be numeric or an ML vector: $c"))
            val target = config("output_column").str
            require(target.trim.nonEmpty && !df.columns.contains(target), s"Output column already exists or is empty: $target")
            table(node, new VectorAssembler().setInputCols(columns.map(FeaturePreparation.quoted)).setOutputCol(target)
              .setHandleInvalid("error").transform(df))
          case "linear_regression" =>
            val df = input(node)
            val label = config("label_column").str
            val features = config("features_column").str
            requireColumns(df, Seq(label, features))
            require(df.schema(label).dataType.isInstanceOf[NumericType], "Regression label must be numeric")
            require(df.schema(features).dataType == SQLDataTypes.VectorType, "Features must be a Spark ML vector")
            require(!df.columns.contains("prediction"), "Reserved column already exists: prediction")
            val estimator = new LinearRegression().setLabelCol(label).setFeaturesCol(features)
              .setFitIntercept(config("fit_intercept").bool).setMaxIter(config("max_iter").num.toInt)
              .setRegParam(config("reg_param").num).setElasticNetParam(config("elastic_net_param").num)
            val model = estimator.fit(df)
            models(node) = model
            if (cached || node == output) {
              require(model.coefficients.size <= MaxPreviewRows - 1, "Too many coefficients for bounded report")
              val estimates = Seq(Seq(Str("Intercept"), finite(model.intercept))) ++
                model.coefficients.toArray.zipWithIndex.map { case (v, i) => Seq(Str(s"feature[$i]"), finite(v)) }
              results = Vector(
                report(spark, "Coefficients", Seq("parameter", "estimate"), estimates),
                report(spark, "Training metrics", Seq("metric", "value"), Seq(
                  Seq(Str("rmse"), finite(model.summary.rootMeanSquaredError)),
                  Seq(Str("r2"), finite(model.summary.r2))))
              )
            }
          case "generalized_linear_regression" | "decision_tree_regressor" | "random_forest_regressor" =>
            models(node) = RegressionSupport.fit(input(node), config)
          case "logistic_regression" | "linear_svc" | "decision_tree_classifier" | "random_forest_classifier" =>
            models(node) = ClassificationSupport.fit(input(node), config)
          case "classification_metrics" =>
            require(cached || node == output, "Classification Metrics is only valid as selected output")
            val metrics = ClassificationSupport.metrics(input(node), config("label_column").str, config("prediction_column").str)
            import scala.jdk.CollectionConverters._
            val schema = new StructType().add("metric", StringType, false).add("value", DoubleType, false)
            results = Vector(preview("Classification metrics",
              spark.createDataFrame(metrics.map { case (name, value) => Row(name, value) }.asJava, schema), MaxPreviewRows))
          case "regression_metrics" =>
            require(cached || node == output, "Regression Metrics is only valid as selected output")
            val metrics = RegressionSupport.metrics(input(node), config("label_column").str, config("prediction_column").str)
            import scala.jdk.CollectionConverters._
            val schema = new StructType().add("metric", StringType, false).add("value", DoubleType, true)
            val rows = metrics.map { case (name, value) => Row(name, value.map(Double.box).orNull) }
            results = Vector(preview("Regression metrics", spark.createDataFrame(rows.asJava, schema), MaxPreviewRows))
          case "predict" | "transform" =>
            val edge = upstreamEdge(node, "model")
            require(edge("from_port").str == "model", "Expected a model output port")
            val model = models.getOrElse(edge("from").str,
              throw new IllegalArgumentException("Missing model dependency"))
            val prediction = config("kind").str == "predict"
            require(prediction == (RegressionSupport.isRegression(model) || ClassificationSupport.isClassifier(model)),
              if (prediction) "Predict requires a regression model or classifier; use Transform for preprocessing"
              else "Transform requires a preprocessing model; use Predict for regression/classification")
            table(node, model.transform(input(node)))
          case "table_preview" =>
            require(cached || node == output, "Preview is only valid as selected output")
            results = Vector(preview("Preview", input(node), config("rows").num.toInt))
          case other => throw new IllegalArgumentException(s"Unsupported node kind: $other")
        }
        if (cached) {
          val expose = node == output || !edges.exists(e => e("from").str == node && order.contains(e("to").str))
          val outputs = tables.toVector.filter(_._1._1 == node).sortBy(_._1._2)
          outputs.foreach { case (_, df) =>
            df.persist(StorageLevel.MEMORY_AND_DISK)
            df.count()
          }
          if (results.isEmpty && expose) {
            results = outputs.map { case ((_, port), df) => preview(s"Preview ($port)", df, 100) }
            if (results.isEmpty && models.contains(node)) results = Vector(
              report(spark, "Model", Seq("node", "status"), Seq(Seq(Str(node), Str("fitted")))))
          }
          require(!expose || results.nonEmpty, s"Node $node produced no result")
          cache.tokens(node) = tokens.getOrElse(node, throw new IllegalArgumentException("Missing cache token"))
          progress(node, "completed")
          // Return bounded terminal reports/previews; intermediate data stays in Spark.
          if (expose) {
            allResults ++= results.map { table => table("name") = Str(s"${table("name").str} [$node]"); table }
            require(ujson.write(Arr.from(allResults)).getBytes(StandardCharsets.UTF_8).length < MaxResponseBytes - 1024,
              "Preview exceeds 8 MiB response limit; run fewer terminal nodes or reduce preview width")
          }
        } else progress(node, "prepared")
      }
      if (cached) results = allResults
      require(results.nonEmpty, "Selected output produced no result tables")
      val response = Obj("protocol_version" -> ProtocolVersion, "request_id" -> id,
        "ok" -> true, "tables" -> Arr.from(results))
      require(ujson.write(response).getBytes(StandardCharsets.UTF_8).length < MaxResponseBytes,
        "Preview exceeds 8 MiB response limit; select fewer columns or rows")
      response
    } catch {
      case NonFatal(error) =>
        if (request.obj.get("operation").exists(_.str == "run_nodes")) activeNode.foreach(cache.remove)
        failure(id, activeNode, error)
    } finally {
      spark.sparkContext.clearJobGroup()
    }
  }
}
