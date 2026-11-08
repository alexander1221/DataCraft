package datacraft

import org.apache.spark.ml.{Pipeline, PipelineModel}
import org.apache.spark.ml.feature.{OneHotEncoder, RFormula, RFormulaModel, StringIndexer}
import org.apache.spark.sql.{Column, DataFrame}
import org.apache.spark.sql.functions.{col, lit, pmod, xxhash64}
import org.apache.spark.sql.types.{NumericType, StringType}

/** Fitted transformations never see the held-out table. Contract: docs/PREPROCESSING.md. */
private[datacraft] object FeaturePreparation {
  private val Buckets = 1L << 53
  def quoted(name: String): String = "`" + name.replace("`", "``") + "`"
  private def exact(name: String): Column = col(quoted(name))

  def requireColumns(df: DataFrame, columns: Seq[String]): Unit = {
    columns.foreach(name => require(df.columns.contains(name), s"Unknown column: $name"))
  }
  private def newColumns(df: DataFrame, columns: Seq[String]): Unit = {
    require(columns.forall(_.trim.nonEmpty) && columns.distinct.size == columns.size,
      "Output columns must be nonempty and distinct")
    columns.foreach(c => require(!df.columns.contains(c), s"Output column already exists: $c"))
  }
  private def training(df: DataFrame): Unit = {
    require(df.limit(1).count() > 0, "Training table is empty")
  }

  def split(df: DataFrame, keys: Seq[String], fraction: Double, seed: Long): (DataFrame, DataFrame) = {
    require(keys.nonEmpty && keys.forall(_.trim.nonEmpty) && keys.distinct.size == keys.size,
      "Split key columns must be nonempty and unique")
    require(fraction.isFinite && fraction > 0 && fraction < 1, "Train fraction must be finite and strictly between 0 and 1")
    require(seed >= 0 && seed <= 4294967295L, "Seed must fit u32")
    requireColumns(df, keys)
    val score = pmod(xxhash64((Seq(lit(seed)) ++ keys.map(exact)): _*), lit(Buckets))
    val cutoff = (fraction * Buckets.toDouble).toLong
    (df.filter(score < lit(cutoff)), df.filter(score >= lit(cutoff)))
  }

  def index(df: DataFrame, columns: Array[String], keepUnknown: Boolean, oneHot: Boolean,
            dropLast: Boolean): PipelineModel = {
    requireColumns(df, columns.toSeq)
    columns.foreach { c =>
      require(df.schema(c).dataType == StringType || df.schema(c).dataType.isInstanceOf[NumericType],
        s"Categorical column must be string or numeric: $c")
    }
    val indices = columns.map(_ + "_idx")
    val vectors = columns.map(_ + "_idx_vec")
    newColumns(df, (if (oneHot) indices ++ vectors else indices).toSeq)
    training(df)
    val indexer = new StringIndexer().setInputCols(columns.map(quoted)).setOutputCols(indices)
      .setStringOrderType("alphabetAsc").setHandleInvalid(if (keepUnknown) "keep" else "error")
    val stages = if (oneHot) {
      Array(indexer, new OneHotEncoder().setInputCols(indices.map(quoted)).setOutputCols(vectors)
        .setDropLast(dropLast).setHandleInvalid("error"))
    } else Array(indexer)
    new Pipeline().setStages(stages).fit(df)
  }

  def design(df: DataFrame, formula: String, features: String, label: String,
             keepUnknown: Boolean): RFormulaModel = {
    require(formula.trim.nonEmpty, "Formula must not be empty")
    require(features != label, "Features and label output columns must differ")
    newColumns(df, Seq(features))
    val response = formula.takeWhile(_ != '~').trim
    require(!df.columns.contains(label) || (response == label && df.schema(label).dataType.isInstanceOf[NumericType]),
      s"Label output would overwrite another column: $label")
    training(df)
    new RFormula().setFormula(formula).setFeaturesCol(features).setLabelCol(label)
      .setStringIndexerOrderType("alphabetAsc")
      .setHandleInvalid(if (keepUnknown) "keep" else "error").fit(df)
  }
}
