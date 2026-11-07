package datacraft

import java.nio.charset.StandardCharsets
import java.nio.file.{Files, Path, Paths}
import java.util.UUID
import org.apache.spark.ml.feature.VectorAssembler
import org.apache.spark.ml.linalg.SQLDataTypes
import org.apache.spark.ml.regression.{LinearRegression, LinearRegressionModel}
import org.apache.spark.sql.{DataFrame, SparkSession}
import org.apache.spark.sql.functions.col
import org.apache.spark.sql.types.NumericType
import scala.collection.mutable
import scala.util.control.NonFatal
import ujson.{Arr, Null, Num, Obj, Str, Value}

/** Local-only worker. stdout is exclusively newline-delimited protocol JSON. */
object Worker {
  val ProtocolVersion = 1
  val MaxPreviewRows = 1000
  val MaxResponseBytes = 8 * 1024 * 1024

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
    try {
      source.getLines().foreach { line =>
        val response = try execute(spark, ujson.read(line)) catch {
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
    val values = bounded.toJSON.collect().map(ujson.read(_))
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

  def execute(spark: SparkSession, request: Value): Value = {
    val id = request.obj.getOrElse("request_id", Null)
    var activeNode: Option[String] = None
    try {
      UUID.fromString(id.str)
      require(request("protocol_version").num == ProtocolVersion, "Unsupported protocol version")
      require(request("operation").str == "run", "Unsupported operation")
      val project = request("project")
      require(project("format_version").num == 1, "Unsupported project format")
      val nodes = project("nodes").arr.map(n => n("id").str -> n("config")).toMap
      require(nodes.size == project("nodes").arr.size, "Duplicate node IDs")
      val edges = project("edges").arr
      val order = request("execution_order").arr.map(_.str)
      val output = request("output_node").str
      require(order.nonEmpty && order.last == output && order.distinct.size == order.size,
        "Invalid execution order")
      val tables = mutable.Map.empty[String, DataFrame]
      val models = mutable.Map.empty[String, LinearRegressionModel]
      var results = Vector.empty[Value]

      def upstream(node: String, port: String): String = {
        val matches = edges.filter(e => e("to").str == node && e("to_port").str == port)
        require(matches.size == 1, s"Expected exactly one input: $node:$port")
        matches.head("from").str
      }
      def input(node: String): DataFrame = tables.getOrElse(upstream(node, "table"),
        throw new IllegalArgumentException(s"Missing table dependency for $node"))

      spark.sparkContext.setJobGroup(id.str, "DataCraft execution", interruptOnCancel = true)
      order.foreach { node =>
        activeNode = Some(node)
        val config = nodes.getOrElse(node, throw new IllegalArgumentException(s"Unknown node $node"))
        config("kind").str match {
          case "csv_source" =>
            val path = localPath(config("path").str)
            tables(node) = spark.read.option("header", config("header").bool)
              .option("inferSchema", config("infer_schema").bool)
              .option("mode", "FAILFAST").csv(path.toString)
          case "parquet_source" =>
            tables(node) = spark.read.parquet(localPath(config("path").str).toString)
          case "filter" => tables(node) = input(node).filter(config("expression").str)
          case "select_columns" =>
            val df = input(node)
            val columns = names(config)
            requireColumns(df, columns.toSeq)
            tables(node) = df.select(columns.map(exactColumn): _*)
          case "limit" =>
            val rows = config("rows").num.toInt
            require(rows > 0, "Limit must be positive")
            tables(node) = input(node).limit(rows)
          case "assemble_features" =>
            val df = input(node)
            val columns = names(config)
            requireColumns(df, columns.toSeq)
            columns.foreach(c => require(df.schema(c).dataType.isInstanceOf[NumericType], s"Non-numeric feature: $c"))
            val target = config("output_column").str
            require(target.trim.nonEmpty && !df.columns.contains(target), s"Output column already exists or is empty: $target")
            tables(node) = new VectorAssembler().setInputCols(columns).setOutputCol(target)
              .setHandleInvalid("error").transform(df)
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
            if (node == output) {
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
          case "predict" =>
            val model = models.getOrElse(upstream(node, "model"),
              throw new IllegalArgumentException("Missing model dependency"))
            tables(node) = model.transform(input(node))
          case "table_preview" =>
            require(node == output, "Preview is only valid as selected output")
            results = Vector(preview("Preview", input(node), config("rows").num.toInt))
          case other => throw new IllegalArgumentException(s"Unsupported node kind: $other")
        }
      }
      require(results.nonEmpty, "Selected output produced no result tables")
      val response = Obj("protocol_version" -> ProtocolVersion, "request_id" -> id,
        "ok" -> true, "tables" -> Arr.from(results))
      require(ujson.write(response).getBytes(StandardCharsets.UTF_8).length < MaxResponseBytes,
        "Preview exceeds 8 MiB response limit; select fewer columns or rows")
      response
    } catch {
      case NonFatal(error) => failure(id, activeNode, error)
    } finally {
      spark.sparkContext.clearJobGroup()
    }
  }
}
