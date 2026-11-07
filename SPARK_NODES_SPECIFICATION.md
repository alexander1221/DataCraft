# Spark Nodes Specification for Node-Based Big Data Editor

**Spark Version:** Minimum 2.0\
**Language:** Spark Scala\
**Execution Environment:** Livy Server / Spark Cluster\

---

## Table of Contents

1. [Common Architecture & Conventions](#common-architecture--conventions)
2. [Data I/O Nodes](#data-io-nodes)
   - [Spark Data (Read)](#spark-data-read)
3. [Preprocessing & Feature Engineering Nodes](#preprocessing--feature-engineering-nodes)
   - [Spark Make Design Matrix](#spark-make-design-matrix)
   - [Spark Index Categoricals](#spark-index-categoricals)
   - [Spark Reference Code Categoricals (One-Hot Encoding)](#spark-reference-code-categoricals-one-hot-encoding)
4. [Feature Selection Nodes](#feature-selection-nodes)
   - [Spark Feature Selection (Chi-Sq)](#spark-feature-selection-chi-sq)
   - [Spark Feature Selection Pipeline](#spark-feature-selection-pipeline)
5. [Regression Models](#regression-models)
   - [Spark Linear Regression](#spark-linear-regression)
   - [Spark Generalized Linear Model (GLM)](#spark-generalized-linear-model-glm)
   - [Spark Decision Tree Regressor](#spark-decision-tree-regressor)
   - [Spark Random Forest Regressor](#spark-random-forest-regressor)
6. [Classification Models](#classification-models)
   - [Spark Logistic Regression](#spark-logistic-regression)
   - [Spark Decision Tree Classifier](#spark-decision-tree-classifier)
   - [Spark Random Forest Classifier](#spark-random-forest-classifier)
   - [Spark SVM Classification](#spark-svm-classification)
7. [Ensemble / Pipeline Nodes](#ensemble--pipeline-nodes)
   - [Spark Random Forest Pipeline (Feature Selection via RF)](#spark-random-forest-pipeline-feature-selection-via-rf)
8. [Template Node](#template-node)
   - [Spark Scala Script (Empty Template)](#spark-scala-script-empty-template)
9. [Common Parameter Patterns](#common-parameter-patterns)
10. [Common Code Patterns](#common-code-patterns)

---

## Common Architecture & Conventions

### Node Structure (GCN XML Format)
Each node is defined as a `GCN` (General Computation Node) with:
- **CLSID:** `c9e2a4fa-3d45-11d4-9ff4-00c04fa0d540` (constant)
- **TYPE:** `scDMAnalysisNode2` (constant)
- **Tabs:** Parameters, Code, Options, Metadata, Spark Options

### Data Flow Tokens (Standardized Across All Nodes)
| Token | Variable Name | Description |
|-------|---------------|-------------|
| Data In | `staIn` | Input DataFrame(s) - array of DataFrames |
| Data Out | `staOut` | Output DataFrame (for transformation nodes) |
| Results | `staResults` | Output results DataFrames (metrics, coefficients, importance) |

### Input/Output Configuration (Options Tab)
| Property | Values | Meaning |
|----------|--------|---------|
| `RequiresInput` | `True`/`False` | Whether node needs upstream connection |
| `HasOutputDataset` | `True`/`False` | Whether node produces `staOut` DataFrame |
| `MaxNumberOfInputs` | 1-5 | Max upstream connections (usually 1 for models, 5 for preprocessing) |

### Spark Options (Standard Across All Nodes)
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `PollingInterval` | Int | 1 | Livy server polling interval (sec), min=1 |
| `Downstream:GetData` | Boolean | true/false | Retrieve data + schema (mind dataset size) |
| `Downstream:TopRows` | Int | 0 | Rows to pull to client (0 = all), min=0 |

### Variable Selection Editor
All variable selectors use `PropertyGridHelper.VariableSelectionEditor` with metadata:
- `GCNDisplayType`: "VariableSelection"
- `VariableType`: 0=Binary, 1=Categorical (String), 2=Continuous (Double)
- `MinVars`/`MaxVars`: Cardinality constraints
- `VariableSelectionTitle`/`Caption`: UI labels

---

## Data I/O Nodes

### Spark Data (Read)

**NodeName:** `Spark Data` / `Spark Read Data`\
**ShortDescription:** Read data from filesystem (HDFS/local) into Spark DataFrame. Supports CSV, JSON, Parquet.\
**RequiresInput:** `False`\
**HasOutputDataset:** `True`\
**MaxNumberOfInputs:** 1\

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints | Description |
|------|------|-------------|----------|----------|---------|-------------|-------------|
| `filePath` | String | Path to file | Data | Yes | "Enter full path to file here..." | - | Full path: `hdfs:///user/data.csv` or `file:///home/user/data.csv` |
| `fileType` | Enum | File type | Data | No | `csv` | csv, json, parquet | File format |
| `getHeader` | Boolean | Header | CSV Options | No | `true` | - | CSV has header row |
| `inferSchema` | Boolean | Infer schema | CSV Options | No | `true` | - | Auto-infer types vs all strings |
| `naDrop` | Boolean | Casewise delete MD | Data | No | `true` | - | Drop rows with any null |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// Reads file in csv, json or parquet from hdfs/filesystem
// Produces spark DataFrame for downstream ML analyses
//
import org.apache.spark.sql._

var outDataFrame = fileType match {
  case "csv" => {
    spark.read.format("csv")
      .option("header", if (getHeader) "true" else "false")
      .option("inferSchema", if (inferSchema) "true" else "false")
      .load(filePath)
  }
  case "parquet" => spark.read.format("parquet").load(filePath)
  case _ => spark.read.format("json").load(filePath)
}

staOut = if (naDrop) outDataFrame.na.drop() else outDataFrame
```

---

## Preprocessing & Feature Engineering Nodes

### Spark Make Design Matrix

**NodeName:** `Spark Make Design Matrix`\
**ShortDescription:** Creates main-effects design matrix using RFormula API.\
**RequiresInput:** `True`\
**HasOutputDataset:** `True`\
**MaxNumberOfInputs:** 1\

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints | Description |
|------|------|-------------|----------|----------|---------|-------------|-------------|
| `depVar` | String (VarSel) | Dependent variable | Variable Selection | No | - | VariableType=2, Min=1, Max=1 | Target column (continuous) |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 | Continuous feature columns |
| `_vsCatPreds` | String (VarSel) | Categorical variables | Variable Selection | No | - | VariableType=1, Min=0 | Categorical feature columns |
| `setIntercept` | Boolean | Include intercept | Parameters | No | `true` | - | Add intercept term |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// Uses RFormula API for main effects design matrix
// Input: depVar + continuous/categorical predictors
// Output: staOut = design matrix DataFrame
//
import org.apache.spark.sql._
import org.apache.spark.ml.feature.RFormula

println("continuos vars:" + _vsConPreds)
println("catvars:" + _vsCatPreds)
if (_vsConPreds.isEmpty && _vsCatPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds.toArray
val catPred = _vsCatPreds.toArray

val labelCol = "label"
val featureVec = "outfeatures"
var rFormula = depVar + " ~ " + (conPred ++ catPred).mkString("+")
if (!setIntercept) rFormula = rFormula + " - 1"

val inputDataFrame = staIn(0)
val formula = new RFormula().setFormula(rFormula).setFeaturesCol(featureVec).setLabelCol(labelCol)
val designMatrix = formula.fit(inputDataFrame).transform(inputDataFrame)

designMatrix.printSchema
staOut = designMatrix
```

---

### Spark Index Categoricals

**NodeName:** `Spark Index Categoricals`\
**ShortDescription:** Converts string columns to numeric indices via StringIndexer. Column `x` → `x_idx`.\
**RequiresInput:** `False` (but accepts up to 5 inputs)\
**HasOutputDataset:** `True`\
**MaxNumberOfInputs:** 5\

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `_vsCatPreds` | String (VarSel) | Categorical variables | Variable Selection | No | - | VariableType=1, Min=0 |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// StringIndexer: string column -> numeric indices (x -> x_idx)
// Output: staOut = transformed DataFrame with indexed columns
//
import org.apache.spark.sql.types.{StructType,StructField,IntegerType,StringType,DoubleType}
import org.apache.spark.ml.feature.StringIndexer
import org.apache.spark.sql._
import org.apache.spark.ml.Pipeline

val inputDataFrame:DataFrame = staIn(0)
println("categorical vars:" + _vsCatPreds)
if (_vsCatPreds.isEmpty) throw new Exception("Empty variable selection")

val catPred = _vsCatPreds.toArray
val catIdx = catPred.map(c => s"${c}_idx")
val indexers = catPred.map(c => new StringIndexer().setInputCol(c).setOutputCol(s"${c}_idx"))

val pipeline = new Pipeline().setStages(indexers)
val designMatrix = pipeline.fit(inputDataFrame).transform(inputDataFrame)

designMatrix.printSchema
staOut = designMatrix
```

---

### Spark Reference Code Categoricals (One-Hot Encoding)

**NodeName:** `Spark Reference Code Categoricals`\
**ShortDescription:** StringIndexer + OneHotEncoder: string column → binary vectors (`x` → `x_idx_vec`).\
**RequiresInput:** `False` (but accepts up to 5 inputs)\
**HasOutputDataset:** `True`\
**MaxNumberOfInputs:** 5\

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `_vsCatPreds` | String (VarSel) | Categorical variables | Variable Selection | No | - | VariableType=1, Min=0 |
| `setDropLast` | Boolean | Drop last category | Parameters | No | `true` | - |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// StringIndexer + OneHotEncoder: string -> binary vectors (x -> x_idx_vec)
// Output: staOut = transformed DataFrame with encoded columns
//
import org.apache.spark.sql.types.{StructType,StructField,IntegerType,StringType,DoubleType}
import org.apache.spark.ml.feature.{OneHotEncoder, StringIndexer, VectorAssembler}
import org.apache.spark.sql._
import org.apache.spark.ml.Pipeline

val inputDataFrame:DataFrame = staIn(0)
println("categorical vars:" + _vsCatPreds)
if (_vsCatPreds.isEmpty) throw new Exception("Empty variable selection")

val catPred = _vsCatPreds.toArray
val catIdx = catPred.map(c => s"${c}_idx")
val catVec = catIdx.map(c => s"${c}_vec")

val indexers = catPred.map(c => new StringIndexer().setInputCol(c).setOutputCol(s"${c}_idx"))
val encoders = catIdx.map(c => new OneHotEncoder().setInputCol(c).setOutputCol(s"${c}_vec").setDropLast(setDropLast))

val pipeline = new Pipeline().setStages(indexers ++ encoders)
val designMatrix = pipeline.fit(inputDataFrame).transform(inputDataFrame)

designMatrix.printSchema
staOut = designMatrix
```

---

## Feature Selection Nodes

### Spark Feature Selection (Chi-Sq)

**NodeName:** `Spark Feature Selection`\
**ShortDescription:** Chi-Squared feature selection per predictor. Continuous → QuantileDiscretizer, Categorical → StringIndexer.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Best Predictors" DataFrame (Parameter, Chi-Squared)

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Continuous dependent | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 |
| `_vsCatPreds` | String (VarSel) | Categorical predictors | Variable Selection | No | - | VariableType=1, Min=0 |
| `numBuckets` | Int | Number of bins | Discretizer Parameters | No | 10 | Min=2, Max=20 |
| `relError` | Double | Relative error | Discretizer Parameters | No | 0.005 | Min=0.001, Max=0.95 |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// Chi-Sq test per (depVar, predictor) pair
// Continuous: QuantileDiscretizer -> chiSqTest
// Categorical: StringIndexer -> chiSqTest
// Output: staResults += sorted Map[Parameter, Chi-Squared]
//
import org.apache.spark.ml.feature.{VectorIndexer,StringIndexer,QuantileDiscretizer,VectorAssembler}
import org.apache.spark.mllib.stat.test.ChiSqTestResult
import org.apache.spark.mllib.regression.LabeledPoint
import org.apache.spark.mllib.stat.Statistics
import org.apache.spark.mllib.linalg._
import org.apache.spark.rdd.RDD
import org.apache.spark.sql._
import scala.collection.breakOut

val conPred = _vsConPreds(0) match { case "" => null; case _ => _vsConPreds.toArray }
val catPred = _vsCatPreds(0) match { case "" => null; case _ => _vsCatPreds.toArray }

println("continuos vars:" + _vsConPreds)
println("catvars:" + _vsCatPreds)
if (_vsConPreds.isEmpty && _vsCatPreds.isEmpty) throw new Exception("Empty variable selection")

val inputDataFrame:DataFrame = staIn(0)
val labelVar = depVar + "_disc"
val featureVec = "feature"
val catFeature = "catFeatureIdx"
val chiSq = scala.collection.mutable.ArrayBuffer.empty[Double]

// Discretize continuous dependent
val ydisc = new QuantileDiscretizer().setInputCol(depVar).setOutputCol(labelVar).setNumBuckets(numBuckets).setRelativeError(relError)
val dataFrame1 = ydisc.fit(inputDataFrame).transform(inputDataFrame).drop(depVar)

// Continuous predictors
if (conPred != null) {
  val chisq1 = for (con <- conPred) yield {
    val dataFrame2 = dataFrame1.select(labelVar, con).na.drop()
    if (dataFrame2.select(con).count > 0) {
      val xdisc = new QuantileDiscretizer().setInputCol(con).setOutputCol(featureVec).setNumBuckets(numBuckets).setRelativeError(relError)
      val xrD = xdisc.fit(dataFrame2).transform(dataFrame2).rdd.map(row => LabeledPoint(row.getAs[Double](labelVar), Vectors.dense(row.getAs[Double](featureVec))))
      Statistics.chiSqTest(xrD)(0).statistic
    } else 0
  }
  chiSq ++= chisq1.toArray
}

// Categorical predictors
if (catPred != null) {
  val chisq2 = for (cat <- catPred) yield {
    val dataFrame3 = dataFrame1.select(labelVar, cat).na.drop()
    if (dataFrame3.select(cat).count > 0) {
      val indexer = new StringIndexer().setInputCol(cat).setOutputCol(catFeature)
      val xrD1 = indexer.fit(dataFrame3).transform(dataFrame3).rdd.map(row => LabeledPoint(row.getAs[Double](labelVar), Vectors.dense(row.getAs[Double](catFeature))))
      Statistics.chiSqTest(xrD1)(0).statistic
    } else 0
  }
  chiSq ++= chisq2.toArray
}

// Combine and sort
val varNames = scala.collection.mutable.ArrayBuffer.empty[String]
if (conPred != null) varNames ++= conPred
if (catPred != null) varNames ++= catPred
val mapChiSq = (varNames.toArray zip chiSq)(breakOut): Map[String,Double]

val chiDf = spark.createDataFrame(Array(mapChiSq.toSeq.sortWith(_._2 > _._2):_*)).toDF("Parameter","Chi-Squared").as("Best Predictors")
staResults += chiDf
```

---

### Spark Feature Selection Pipeline

**NodeName:** `Spark Feature Selection Pipeline`\
**ShortDescription:** ML Pipeline with ChiSqSelector for top-k feature selection.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Best Predictors" DataFrame (Parameter, Chi-Squared) for selected features

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Continuous dependent | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 |
| `_vsCatPreds` | String (VarSel) | Categorical predictors | Variable Selection | No | - | VariableType=1, Min=0 |
| `numBuckets` | Int | Number of bins | Discretizer Parameters | No | 10 | Min=2, Max=20 |
| `relError` | Double | Relative error | Discretizer Parameters | No | 0.005 | Min=0.001, Max=0.95 |
| `numSelectedFeatures` | Int | No. of Selected Features | ChiSqSelector Parameters | Yes | 100 | Min=2 |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// ML Pipeline: QuantileDiscretizer + StringIndexer + VectorAssembler + ChiSqSelector
// Output: staResults += top-k features with Chi-Sq stats
//
import org.apache.spark.ml.feature.{StringIndexer,VectorAssembler,QuantileDiscretizer,ChiSqSelector,ChiSqSelectorModel}
import org.apache.spark.mllib.linalg.{Vectors => OldVectors}
import org.apache.spark.mllib.stat.test.ChiSqTestResult
import org.apache.spark.mllib.regression.LabeledPoint
import org.apache.spark.mllib.stat.Statistics
import org.apache.spark.ml.linalg.DenseVector
import org.apache.spark.ml.Pipeline
import org.apache.spark.sql._
import org.apache.spark.rdd.RDD

val conPred = _vsConPreds(0) match { case "" => null; case _ => _vsConPreds.toArray }
val catPred = _vsCatPreds(0) match { case "" => null; case _ => _vsCatPreds.toArray }

println("continuos vars:" + _vsConPreds)
println("catvars:" + _vsCatPreds)
if (_vsConPreds.isEmpty && _vsCatPreds.isEmpty) throw new Exception("Empty variable selection")

val inputDataFrame:DataFrame = staIn(0)
val labelVar = depVar + "_disc"
val featureVec = "features"
val selFeatures = "selectedFeatures"
var chiSqArr: Array[(String, Double)] = null

if (conPred != null && catPred != null) {
  val catIdx = catPred.map(c => s"${c}_idx")
  val conDisc = conPred.map(c => s"${c}_disc")
  val indexers = catPred.map(c => new StringIndexer().setInputCol(c).setOutputCol(s"${c}_idx"))
  val discretizers = (Array(depVar) ++ conPred).map(c => new QuantileDiscretizer().setInputCol(c).setOutputCol(s"${c}_disc").setNumBuckets(numBuckets).setRelativeError(relError))
  val assembler = new VectorAssembler().setInputCols(catIdx ++ conDisc).setOutputCol(featureVec)
  val selector = new ChiSqSelector().setNumTopFeatures(numSelectedFeatures).setFeaturesCol(featureVec).setLabelCol(labelVar).setOutputCol(selFeatures)
  val pipeline = new Pipeline().setStages(discretizers ++ indexers ++ Array(assembler, selector))
  val model = pipeline.fit(inputDataFrame)
  val varNames = catPred ++ conPred
  val nStages = model.stages.size - 1
  val selectedVars = model.stages(nStages).asInstanceOf[ChiSqSelectorModel].selectedFeatures.sortWith(_ < _).map(i => varNames(i))
  val rD = model.transform(inputDataFrame).rdd.map(row => LabeledPoint(row.getAs[Double](labelVar), OldVectors.fromML(row.getAs[DenseVector](selFeatures))))
  val chiSq = Statistics.chiSqTest(rD).map(x => x.statistic)
  chiSqArr = selectedVars zip chiSq
} else if (conPred != null) {
  val conDisc = conPred.map(c => s"${c}_disc")
  val discretizers = (Array(depVar) ++ conPred).map(c => new QuantileDiscretizer().setInputCol(c).setOutputCol(s"${c}_disc").setNumBuckets(numBuckets).setRelativeError(relError))
  val assembler = new VectorAssembler().setInputCols(conDisc).setOutputCol(featureVec)
  val selector = new ChiSqSelector().setNumTopFeatures(numSelectedFeatures).setFeaturesCol(featureVec).setLabelCol(labelVar).setOutputCol(selFeatures)
  val pipeline = new Pipeline().setStages(discretizers ++ Array(assembler, selector))
  val model = pipeline.fit(inputDataFrame)
  val nStages = model.stages.size - 1
  val selectedVars = model.stages(nStages).asInstanceOf[ChiSqSelectorModel].selectedFeatures.sortWith(_ < _).map(i => conPred(i))
  val rD = model.transform(inputDataFrame).rdd.map(row => LabeledPoint(row.getAs[Double](labelVar), OldVectors.fromML(row.getAs[DenseVector](selFeatures))))
  val chiSq = Statistics.chiSqTest(rD).map(x => x.statistic)
  chiSqArr = selectedVars zip chiSq
} else if (catPred != null) {
  val catIdx = catPred.map(c => s"${c}_idx")
  val indexers = catPred.map(c => new StringIndexer().setInputCol(c).setOutputCol(s"${c}_idx"))
  val discretizers = new QuantileDiscretizer().setInputCol(depVar).setOutputCol(labelVar).setNumBuckets(numBuckets).setRelativeError(relError)
  val assembler = new VectorAssembler().setInputCols(catIdx).setOutputCol(featureVec)
  val selector = new ChiSqSelector().setNumTopFeatures(numSelectedFeatures).setFeaturesCol(featureVec).setLabelCol(labelVar).setOutputCol(selFeatures)
  val pipeline = new Pipeline().setStages(Array(discretizers) ++ indexers ++ Array(assembler, selector))
  val model = pipeline.fit(inputDataFrame)
  val nStages = model.stages.size - 1
  val selectedVars = model.stages(nStages).asInstanceOf[ChiSqSelectorModel].selectedFeatures.sortWith(_ < _).map(i => catPred(i))
  val rD = model.transform(inputDataFrame).rdd.map(row => LabeledPoint(row.getAs[Double](labelVar), OldVectors.fromML(row.getAs[DenseVector](selFeatures))))
  val chiSq = Statistics.chiSqTest(rD).map(x => x.statistic)
  chiSqArr = selectedVars zip chiSq
}

val chiDf = spark.createDataFrame(chiSqArr).toDF("Parameter","Chi-Squared").as("Best Predictors")
staResults += chiDf
```

---

## Regression Models

### Spark Linear Regression

**NodeName:** `Spark Linear Regression`\
**ShortDescription:** Linear regression with elastic net regularization.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Parameter estimates" + "Goodness of Fit" DataFrames

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Continuous dependent | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=1 |
| `setIntercept` | Boolean | Fit intercept | Model Parameters | No | `true` | - |
| `setTol` | Int | Convergence tol. 1E- | Model Parameters | Yes | 6 | Min=0 |
| `elasticNetParam` | Double | Elastic net param | Model Parameters | Yes | 1 | Min=0, Max=1 |
| `maxIter` | Int | Max. iterations | Model Parameters | Yes | 10 | Min=0 |
| `regParam` | Double | Regularization param | Model Parameters | Yes | 0 | Min=0 |
| `setSolver` | Enum | Algorithm | Model Parameters | No | `auto` | l_bfgs, normal, auto |
| `weightCol` | String (VarSel) | Weight | Variable Selection | No | - | VariableType=2, Min=0, Max=1 |
| `standardizeData` | Boolean | Standardize data | Model Parameters | No | `true` | - |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// LinearRegression with RFormula for feature assembly
// Output: staResults += coefficients, R2, RMSE, iterations
//
import org.apache.spark.ml.regression.LinearRegression
import org.apache.spark.ml.attribute.AttributeGroup
import org.apache.spark.ml.feature.RFormula
import org.apache.spark.sql._
import scala.collection.mutable._

println("continuos vars:" + _vsConPreds)
if (_vsConPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds.toArray
val solver = setSolver match { case "l_bfgs" => "l-bfgs"; case _ => setSolver }

val labelCol = "label"
val featureVec = "features"
var rFormula = depVar + " ~ " + conPred.mkString("+")
if (!setIntercept) rFormula = rFormula + " - 1"

val inputDataFrame:DataFrame = staIn(0)
val formula = new RFormula().setFormula(rFormula).setFeaturesCol(featureVec).setLabelCol(labelCol)
val lr = weightCol match {
  case "" => new LinearRegression().setFitIntercept(setIntercept).setMaxIter(maxIter).setRegParam(regParam).setElasticNetParam(elasticNetParam).setTol(setTol).setStandardization(standardizeData).setSolver(solver)
  case _ => new LinearRegression().setFitIntercept(setIntercept).setMaxIter(maxIter).setRegParam(regParam).setElasticNetParam(elasticNetParam).setTol(setTol).setStandardization(standardizeData).setSolver(solver).setWeightCol(weightCol)
}

val dm = weightCol match {
  case "" => formula.fit(inputDataFrame).transform(inputDataFrame).select(labelCol, featureVec)
  case _ => formula.fit(inputDataFrame).transform(inputDataFrame).select(labelCol, featureVec, weightCol)
}

val lrModel = lr.fit(dm)
val modelSummary = lrModel.summary

// Coefficients
val attrs = AttributeGroup.fromStructField(dm.schema(1))
val pN = attrs.attributes.get.map(_.name.get)
val paramNames = Array("Intercept") ++ pN
val paramVals = Array(lrModel.intercept) ++ lrModel.coefficients.toArray
val paramDf = spark.createDataFrame(paramNames zip paramVals).toDF("Parameter","Estimate").as("Parameter estimates")
staResults += paramDf

// Goodness of fit
val gofStats = ArrayBuffer(modelSummary.totalIterations, modelSummary.r2, modelSummary.rootMeanSquaredError)
val gofLabels = Array("# iterations","R Squared","RMSE")
val gofDf = spark.createDataFrame(gofLabels zip gofStats).toDF("Statistic","Value").as("Goodness of Fit")
staResults += gofDf

println(s"r2: ${modelSummary.r2}")
```

---

### Spark Generalized Linear Model (GLM)

**NodeName:** `Spark Generalized Linear Model`\
**ShortDescription:** GLM with multiple families (gaussian, binomial, poisson, gamma) and link functions.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Parameter estimates" + "Goodness of Fit" DataFrames

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Dependent variable | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=1 |
| `setIntercept` | Boolean | Fit intercept | Model Parameters | No | `true` | - |
| `setTol` | Int | Convergence tol. 1E- | Model Parameters | Yes | 6 | Min=0 |
| `setFamily` | Enum | Family | Model Parameters | No | `gaussian` | gaussian, binomial, poisson, gamma |
| `maxIter` | Int | Max. iterations | Model Parameters | Yes | 10 | Min=0 |
| `regParam` | Double | Regularization param | Model Parameters | Yes | 0 | Min=0 |
| `weightCol` | String (VarSel) | Weight | Variable Selection | No | - | VariableType=2, Min=0, Max=1 |
| `setLink` | String | Link | Model Parameters | Yes | `log` | See description for valid combos |

**Valid Family/Link Combinations:**
- Gaussian: identity, log, inverse
- Binomial: logit, probit, cloglog
- Poisson: log, identity, sqrt
- Gamma: inverse, identity, log

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// GeneralizedLinearRegression with RFormula
// Output: coefficients + std errors + t + p-values, dispersion, deviance, AIC
//
import org.apache.spark.ml.regression.GeneralizedLinearRegression
import org.apache.spark.ml.attribute.AttributeGroup
import org.apache.spark.ml.feature.RFormula
import org.apache.spark.sql._
import scala.collection.mutable._

val conPred = _vsConPreds
println("continuos vars:" + _vsConPreds)
if (_vsConPreds.isEmpty) throw new Exception("Empty variable selection")

val labelCol = "label"
val featureVec = "features"
var rFormula = depVar + " ~ " + conPred.mkString("+")
if (!setIntercept) rFormula = rFormula + " - 1"

val inputDataFrame:DataFrame = staIn(0)
val formula = new RFormula().setFormula(rFormula).setFeaturesCol(featureVec).setLabelCol(labelCol)
val lr = weightCol match {
  case "" => new GeneralizedLinearRegression().setFamily(setFamily).setLink(setLink).setFitIntercept(setIntercept).setMaxIter(maxIter).setRegParam(regParam).setTol(setTol)
  case _ => new GeneralizedLinearRegression().setFamily(setFamily).setLink(setLink).setFitIntercept(setIntercept).setMaxIter(maxIter).setRegParam(regParam).setTol(setTol).setWeightCol(weightCol)
}

val dm = weightCol match {
  case "" => formula.fit(inputDataFrame).transform(inputDataFrame).select(labelCol, featureVec)
  case _ => formula.fit(inputDataFrame).transform(inputDataFrame).select(labelCol, featureVec, weightCol)
}

val lrModel = lr.fit(dm)
val summary = lrModel.summary

// Coefficients with stats
val attrs = AttributeGroup.fromStructField(dm.schema(1))
val pN = attrs.attributes.get.map(_.name.get)
val paramNames = pN ++ Array("Intercept")
val paramVals = lrModel.coefficients.toArray ++ Array(lrModel.intercept)
val paramMap = paramNames.zip(paramVals).zip(summary.coefficientStandardErrors).zip(summary.tValues).zip(summary.pValues).map{ case ((((a, b), c), d), e) => (a,b,c,d,e) }
val paramDf = spark.createDataFrame(paramMap).toDF("Parameter","Estimate","Standard Error","t","p").as("Parameter estimates")
staResults += paramDf

// GOF
val gofStats = ArrayBuffer(summary.dispersion, summary.nullDeviance, summary.residualDegreeOfFreedomNull, summary.deviance, summary.residualDegreeOfFreedom, summary.aic)
val gofLabels = Array("Dispersion","Null Deviance","Residual df Null Deviance","Deviance","Residual df Deviance","AIC")
val gofDf = spark.createDataFrame(gofLabels zip gofStats).toDF("Statistic","Value").as("Goodness of Fit")
staResults += gofDf
```

---

### Spark Decision Tree Regressor

**NodeName:** `Spark Decision Tree Regressor`\
**ShortDescription:** Single decision tree for regression.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Predictor Importance" DataFrame; model saving to HDFS

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Continuous dependent | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 |
| `setMaxDepth` | Int | Max. tree depth | Model Parameters | Yes | 5 | Min=0 |
| `setMinInstancesPerNode` | Int | Min. Instance per node | Model Parameters | Yes | 1 | Min=1 |
| `setMinInfoGain` | Double | Min. Info. gain | Model Parameters | Yes | 0 | Min=0 |
| `setSeed` | Int | Seed | Model Parameters | Yes | 0 | Min=0 |
| `setMaxBins` | Int | Max. bins | Model Parameters | Yes | 32 | Min=2 |
| `setTestPct` | Double | Test data fraction | Model Parameters | Yes | 0.3 | Min=0, Max=0.5 |
| `saveModel` | Boolean | Save model | Save Model | No | `true` | - |
| `savePath` | String | HDFS path to save to | Save Model | Yes | `hdfs:///user/statistica` | - |
| `saveFile` | String | Model name | Save Model | Yes | `treeModel` | - |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// DecisionTreeRegressor with train/test split
// Output: feature importance, RMSE, model saved to HDFS
//
import org.apache.spark.ml.Pipeline
import org.apache.spark.sql._
import org.apache.spark.ml.feature.VectorAssembler
import org.apache.spark.ml.evaluation.RegressionEvaluator
import org.apache.spark.ml.regression.{DecisionTreeRegressor,DecisionTreeRegressionModel}
import scala.collection.breakOut

println("continuos vars:" + _vsConPreds)
if (_vsConPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds.toArray
val trainPct = 1.0 - setTestPct
val inputDataFrame:DataFrame = staIn(0)
val Array(trainingData, testData) = inputDataFrame.randomSplit(Array(trainPct, setTestPct))

val assembler = new VectorAssembler().setInputCols(conPred).setOutputCol("features")
val dt = new DecisionTreeRegressor().setLabelCol(depVar).setFeaturesCol("features").setMaxBins(setMaxBins).setMaxDepth(setMaxDepth).setMinInfoGain(setMinInfoGain).setMinInstancesPerNode(setMinInstancesPerNode).setSeed(setSeed)
val pipeline = new Pipeline().setStages(Array(assembler, dt))
val model = pipeline.fit(trainingData)
val predictions = model.transform(testData)

// Feature importance
val nStages = model.stages.size - 1
val importanceVec = model.stages(nStages).asInstanceOf[DecisionTreeRegressionModel].featureImportances.toSparse
val selVars = importanceVec.indices.map(i => conPred(i))
val mapImportance = (selVars zip importanceVec.values)(breakOut): Map[String,Double]
val imDf = spark.createDataFrame(Array(mapImportance.toSeq.sortWith(_._2 > _._2):_*)).toDF("Parameter","Relative Importance").as("Predictor Importance")
staResults += imDf

println(model.stages(nStages).asInstanceOf[DecisionTreeRegressionModel].toDebugString)

// Evaluation
val evaluator = new RegressionEvaluator().setLabelCol(depVar).setPredictionCol("prediction").setMetricName("rmse")
val rmse = evaluator.evaluate(predictions)
println("Root Mean Squared Error (RMSE) on test data = " + rmse)

// Save model
if (saveModel) {
  val saveStr = savePath + "/" + saveFile
  model.stages(nStages).asInstanceOf[DecisionTreeRegressionModel].save(saveStr)
}
```

---

### Spark Random Forest Regressor

**NodeName:** `Spark Random Forest Regressor`\
**ShortDescription:** Random Forest for regression with bootstrapping.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Predictor Importance"; model saving to HDFS

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Continuous dependent | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 |
| `setMaxDepth` | Int | Max. tree depth | Model Parameters | Yes | 5 | Min=0 |
| `setMinInstancesPerNode` | Int | Min. Instance per node | Model Parameters | Yes | 1 | Min=1 |
| `setMinInfoGain` | Double | Min. Info. gain | Model Parameters | Yes | 0 | Min=0 |
| `setSeed` | Int | Seed | Model Parameters | Yes | 0 | Min=0 |
| `setMaxBins` | Int | Max. bins | Model Parameters | Yes | 32 | Min=2 |
| `setTestPct` | Double | Test data fraction | Model Parameters | Yes | 0.3 | Min=0, Max=0.5 |
| `setNumTrees` | Int | No. of trees | Model Parameters | Yes | 20 | Min=1 |
| `setSubsamplingRate` | Double | Subsampling rate | Model Parameters | Yes | 1 | Min=0, Max=1 |
| `saveModel` | Boolean | Save model | Save Model | No | `true` | - |
| `savePath` | String | HDFS path to save to | Save Model | Yes | `hdfs:///user/statistica` | - |
| `saveFile` | String | Model name | Save Model | Yes | `treeModel` | - |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// RandomForestRegressor with train/test split
// Output: feature importance, RMSE, model saved to HDFS
//
import org.apache.spark.sql._
import org.apache.spark.ml.Pipeline
import org.apache.spark.ml.feature.VectorAssembler
import org.apache.spark.ml.evaluation.RegressionEvaluator
import org.apache.spark.ml.regression.{RandomForestRegressor,RandomForestRegressionModel}
import scala.collection.breakOut

println("continuos vars:" + _vsConPreds)
if (_vsConPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds.toArray
val trainPct = 1.0 - setTestPct
val featuresCol = "features"
val inputDataFrame:DataFrame = staIn(0)
val Array(trainingData, testData) = inputDataFrame.randomSplit(Array(trainPct, setTestPct))

val assembler = new VectorAssembler().setInputCols(conPred).setOutputCol(featuresCol)
val rf = new RandomForestRegressor().setLabelCol(depVar).setFeaturesCol(featuresCol).setMaxBins(setMaxBins).setMaxDepth(setMaxDepth).setMinInfoGain(setMinInfoGain).setMinInstancesPerNode(setMinInstancesPerNode).setSubsamplingRate(setSubsamplingRate).setNumTrees(setNumTrees).setSeed(setSeed)
val pipeline = new Pipeline().setStages(Array(assembler, rf))
val model = pipeline.fit(trainingData)
val predictions = model.transform(testData)

// Feature importance
val nStages = model.stages.size - 1
val importanceVec = model.stages(nStages).asInstanceOf[RandomForestRegressionModel].featureImportances.toSparse
val selVars = importanceVec.indices.map(i => conPred(i))
val mapImportance = (selVars zip importanceVec.values)(breakOut): Map[String,Double]
val imDf = spark.createDataFrame(Array(mapImportance.toSeq.sortWith(_._2 > _._2):_*)).toDF("Parameter","Relative Importance").as("Predictor Importance")
staResults += imDf

model.stages(nStages).asInstanceOf[RandomForestRegressionModel].toDebugString

// Evaluation
val evaluator = new RegressionEvaluator().setLabelCol(depVar).setPredictionCol("prediction").setMetricName("rmse")
val rmse = evaluator.evaluate(predictions)
println("Root Mean Squared Error (RMSE) on test data = " + rmse)

// Save model
if (saveModel) {
  val saveStr = savePath + "/" + saveFile
  model.stages(nStages).asInstanceOf[RandomForestRegressionModel].save(saveStr)
}
```

---

## Classification Models

### Spark Logistic Regression

**NodeName:** `Spark Logistic Regression`\
**ShortDescription:** Binomial logistic regression with elastic net.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Parameter estimates", "ROC", "Precision-Recall curve" DataFrames

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Binary dependent | Variable Selection | No | - | VariableType=0, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=1 |
| `setIntercept` | Boolean | Fit intercept | Model Parameters | No | `true` | - |
| `setTol` | Double | Convergence tolerance | Model Parameters | Yes | 0.0001 | Min=0 |
| `elasticNetParam` | Double | Elastic net param | Model Parameters | Yes | 1 | Min=0, Max=1 |
| `maxIter` | Int | Max. iterations | Model Parameters | Yes | 10 | Min=0 |
| `regParam` | Double | Regularization parameter | Model Parameters | Yes | 0 | Min=0 |
| `thrParam` | Double | Threshold | Model Parameters | Yes | 0.5 | Min=0, Max=1 |
| `weightCol` | String (VarSel) | Weight | Variable Selection | No | - | VariableType=2, Min=0, Max=1 |
| `standardizeData` | Boolean | Standardize data | Model Parameters | No | `true` | - |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// LogisticRegression with RFormula
// Output: coefficients, ROC curve, PR curve, AUC
//
import org.apache.spark.ml.classification.{BinaryLogisticRegressionSummary, LogisticRegression}
import org.apache.spark.ml.attribute.AttributeGroup
import org.apache.spark.ml.feature.RFormula
import org.apache.spark.ml.Pipeline
import org.apache.spark.sql._

println("continuos vars:" + _vsConPreds)
if (_vsConPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds.toArray
val labelCol = "label"
val featureVec = "features"
var rFormula = depVar + " ~ " + conPred.mkString("+")
if (!setIntercept) rFormula = rFormula + " - 1"

val inputDataFrame:DataFrame = staIn(0)
val formula = new RFormula().setFormula(rFormula).setFeaturesCol(featureVec).setLabelCol(labelCol)
val lr = weightCol match {
  case "" => new LogisticRegression().setFitIntercept(setIntercept).setMaxIter(maxIter).setRegParam(regParam).setElasticNetParam(elasticNetParam).setThreshold(thrParam).setTol(setTol).setStandardization(standardizeData)
  case _ => new LogisticRegression().setFitIntercept(setIntercept).setMaxIter(maxIter).setRegParam(regParam).setElasticNetParam(elasticNetParam).setThreshold(thrParam).setTol(setTol).setStandardization(standardizeData).setWeightCol(weightCol)
}

val dm = weightCol match {
  case "" => formula.fit(inputDataFrame).transform(inputDataFrame).select(labelCol, featureVec)
  case _ => formula.fit(inputDataFrame).transform(inputDataFrame).select(labelCol, featureVec, weightCol)
}

val lrModel = lr.fit(dm)

// Coefficients
val attrs = AttributeGroup.fromStructField(dm.schema(1))
val pN = attrs.attributes.get.map(_.name.get)
val paramNames = Array("Intercept") ++ pN
val paramVals = Array(lrModel.intercept) ++ lrModel.coefficients.toArray
val paramDf = spark.createDataFrame(paramNames zip paramVals).toDF("Parameter","Estimate").as("Parameter estimates")
staResults += paramDf

// ROC & PR curves
val modelSummary = lrModel.summary.asInstanceOf[BinaryLogisticRegressionSummary]
staResults += modelSummary.roc.as("ROC")
staResults += modelSummary.pr.as("Precision-Recall curve")
println(s"areaUnderROC: ${modelSummary.areaUnderROC}")
```

---

### Spark Decision Tree Classifier

**NodeName:** `Spark Decision Tree Classifier`\
**ShortDescription:** Single decision tree for classification.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Predictor Importance"; model saving to HDFS

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Categorical dependent | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 |
| `setMaxDepth` | Int | Max. tree depth | Model Parameters | Yes | 5 | Min=0 |
| `setMinInstancesPerNode` | Int | Min. Instance per node | Model Parameters | Yes | 1 | Min=1 |
| `setMinInfoGain` | Double | Min. Info. gain | Model Parameters | Yes | 0 | Min=0 |
| `setSeed` | Int | Seed | Model Parameters | Yes | 0 | Min=0 |
| `setMaxBins` | Int | Max. bins | Model Parameters | Yes | 32 | Min=2 |
| `setTestPct` | Double | Test data fraction | Model Parameters | Yes | 0.3 | Min=0, Max=0.5 |
| `setImpurity` | Enum | Impurity | Model Parameters | No | `gini` | gini, entropy |
| `saveModel` | Boolean | Save model | Save Model | No | `true` | - |
| `savePath` | String | HDFS path to save to | Save Model | Yes | `hdfs:///user/statistica` | - |
| `saveFile` | String | Model name | Save Model | Yes | `treeModel` | - |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// DecisionTreeClassifier with StringIndexer for label, train/test split
// Output: feature importance, accuracy, model saved to HDFS
//
import org.apache.spark.ml.Pipeline
import org.apache.spark.sql._
import org.apache.spark.ml.feature.{VectorAssembler,IndexToString,StringIndexer}
import org.apache.spark.ml.classification.{DecisionTreeClassifier, DecisionTreeClassificationModel}
import org.apache.spark.ml.evaluation.MulticlassClassificationEvaluator
import scala.collection.breakOut

println("continuos vars:" + _vsConPreds)
if (_vsConPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds.toArray
val trainPct = 1.0 - setTestPct
val featuresCol = "features"
val labelCol = "indexedLabel"
val predCol = "prediction"
val predLabel = "predictedLabel"
val evalMetric = "accuracy"

val inputDataFrame:DataFrame = staIn(0)
val labelIndexer = new StringIndexer().setInputCol(depVar).setOutputCol(labelCol).fit(inputDataFrame)
val Array(trainingData, testData) = inputDataFrame.randomSplit(Array(trainPct, setTestPct))

val assembler = new VectorAssembler().setInputCols(conPred).setOutputCol(featuresCol)
val dt = new DecisionTreeClassifier().setLabelCol(labelCol).setFeaturesCol(featuresCol).setMaxBins(setMaxBins).setMaxDepth(setMaxDepth).setMinInfoGain(setMinInfoGain).setMinInstancesPerNode(setMinInstancesPerNode).setImpurity(setImpurity).setSeed(setSeed)
val labelConverter = new IndexToString().setInputCol(predCol).setOutputCol(predLabel).setLabels(labelIndexer.labels)

val pipeline = new Pipeline().setStages(Array(labelIndexer, assembler, dt, labelConverter))
val model = pipeline.fit(trainingData)
val predictions = model.transform(testData)

// Feature importance
val nStages = model.stages.size - 2
val importanceVec = model.stages(nStages).asInstanceOf[DecisionTreeClassificationModel].featureImportances.toSparse
val selVars = importanceVec.indices.map(i => conPred(i))
val mapImportance = (selVars zip importanceVec.values)(breakOut): Map[String,Double]
val imDf = spark.createDataFrame(Array(mapImportance.toSeq.sortWith(_._2 > _._2):_*)).toDF("Parameter","Relative Importance").as("Predictor Importance")
staResults += imDf

println(model.stages(nStages).asInstanceOf[DecisionTreeClassificationModel].toDebugString)

// Evaluation
val evaluator = new MulticlassClassificationEvaluator().setLabelCol(labelCol).setPredictionCol(predCol).setMetricName(evalMetric)
val accuracy = evaluator.evaluate(predictions)
println("Test Error = " + (1.0 - accuracy))

// Save model
if (saveModel) {
  val saveStr = savePath + "/" + saveFile
  model.stages(nStages).asInstanceOf[DecisionTreeClassificationModel].save(saveStr)
}
```

---

### Spark Random Forest Classifier

**NodeName:** `Spark Random Forest Classifier`\
**ShortDescription:** Random Forest classification with bootstrapping.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Predictor Importance"; model saving to HDFS

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Categorical dependent | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 |
| `setMaxDepth` | Int | Max. tree depth | Model Parameters | Yes | 5 | Min=0 |
| `setMinInstancesPerNode` | Int | Min. Instance per node | Model Parameters | Yes | 1 | Min=1 |
| `setMinInfoGain` | Double | Min. Info. gain | Model Parameters | Yes | 0 | Min=0 |
| `setSeed` | Int | Seed | Model Parameters | Yes | 0 | Min=0 |
| `setMaxBins` | Int | Max. bins | Model Parameters | Yes | 32 | Min=2 |
| `setTestPct` | Double | Test data fraction | Model Parameters | Yes | 0.3 | Min=0, Max=0.5 |
| `setNumTrees` | Int | No. of trees | Model Parameters | Yes | 20 | Min=1 |
| `setSubsamplingRate` | Double | Subsampling rate | Model Parameters | Yes | 1 | Min=0, Max=1 |
| `setImpurity` | Enum | Impurity | Model Parameters | No | `gini` | gini, entropy |
| `saveModel` | Boolean | Save model | Save Model | No | `true` | - |
| `savePath` | String | HDFS path to save to | Save Model | Yes | `hdfs:///user/statistica` | - |
| `saveFile` | String | Model name | Save Model | Yes | `treeModel` | - |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// RandomForestClassifier with StringIndexer, train/test split
// Output: feature importance, accuracy, model saved to HDFS
//
import org.apache.spark.sql._
import org.apache.spark.ml.Pipeline
import org.apache.spark.ml.feature.{VectorAssembler,IndexToString,StringIndexer}
import org.apache.spark.ml.classification.{RandomForestClassifier,RandomForestClassificationModel}
import org.apache.spark.ml.evaluation.MulticlassClassificationEvaluator
import scala.collection.breakOut

println("continuos vars:" + _vsConPreds)
if (_vsConPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds.toArray
val trainPct = 1.0 - setTestPct
val featuresCol = "features"
val labelCol = "indexedLabel"
val predCol = "prediction"
val predLabel = "predictedLabel"
val evalMetric = "accuracy"

val inputDataFrame:DataFrame = staIn(0)
val labelIndexer = new StringIndexer().setInputCol(depVar).setOutputCol(labelCol).fit(inputDataFrame)
val Array(trainingData, testData) = inputDataFrame.randomSplit(Array(trainPct, setTestPct))

val assembler = new VectorAssembler().setInputCols(conPred).setOutputCol(featuresCol)
val rf = new RandomForestClassifier().setLabelCol(labelCol).setFeaturesCol(featuresCol).setMaxBins(setMaxBins).setMaxDepth(setMaxDepth).setMinInfoGain(setMinInfoGain).setMinInstancesPerNode(setMinInstancesPerNode).setSubsamplingRate(setSubsamplingRate).setImpurity(setImpurity).setNumTrees(setNumTrees).setSeed(setSeed)
val labelConverter = new IndexToString().setInputCol(predCol).setOutputCol(predLabel).setLabels(labelIndexer.labels)

val pipeline = new Pipeline().setStages(Array(labelIndexer, assembler, rf, labelConverter))
val model = pipeline.fit(trainingData)
val predictions = model.transform(testData)

// Feature importance
val nStages = model.stages.size - 2
val importanceVec = model.stages(nStages).asInstanceOf[RandomForestClassificationModel].featureImportances.toSparse
val selVars = importanceVec.indices.map(i => conPred(i))
val mapImportance = (selVars zip importanceVec.values)(breakOut): Map[String,Double]
val imDf = spark.createDataFrame(Array(mapImportance.toSeq.sortWith(_._2 > _._2):_*)).toDF("Parameter","Relative Importance").as("Predictor Importance")
staResults += imDf

model.stages(nStages).asInstanceOf[RandomForestClassificationModel].toDebugString

// Evaluation
val evaluator = new MulticlassClassificationEvaluator().setLabelCol(labelCol).setPredictionCol(predCol).setMetricName(evalMetric)
val accuracy = evaluator.evaluate(predictions)
println("Test Error = " + (1.0 - accuracy))

// Save model
if (saveModel) {
  val saveStr = savePath + "/" + saveFile
  model.stages(nStages).asInstanceOf[RandomForestClassificationModel].save(saveStr)
}
```

---

### Spark SVM Classification

**NodeName:** `Spark SVM Classification`\
**ShortDescription:** Linear SVM with SGD (MLlib RDD API). L1 regularization.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Weights", "Goodness of Fit(test set)" (AUC), "Scores (test set)" DataFrames

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Binary dependent | Variable Selection | No | - | VariableType=0, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 |
| `maxIter` | Int | Max. iterations | Model Parameters | Yes | 100 | Min=0 |
| `regParam` | Double | Regularization parameter | Model Parameters | Yes | 0.5 | Min=0 |
| `testPct` | Double | Test data fraction | Model Parameters | Yes | 0.4 | Min=0.1, Max=0.5 |
| `setSeed` | Int | Seed | Model Parameters | Yes | 11 | - |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// SVMWithSGD (MLlib RDD API) with L1Updater
// Output: weights, AUC, scores on test set
//
import org.apache.spark.mllib.evaluation.BinaryClassificationMetrics
import org.apache.spark.mllib.classification.{SVMModel, SVMWithSGD}
import org.apache.spark.mllib.linalg.{Vectors => OldVectors}
import org.apache.spark.ml.attribute.AttributeGroup
import org.apache.spark.mllib.optimization.L1Updater
import org.apache.spark.mllib.regression.LabeledPoint
import org.apache.spark.ml.linalg.DenseVector
import org.apache.spark.ml.feature.RFormula
import org.apache.spark.sql._

println("continuos vars:" + _vsConPreds)
if (_vsConPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds.toArray
val labelCol = "label"
val featureVec = "features"
var rFormula = depVar + " ~ " + conPred.mkString("+")
rFormula = rFormula + " - 1"
val trainPct = 1.0 - testPct

val inputDataFrame = staIn(0)
val splits = inputDataFrame.randomSplit(Array(trainPct, testPct), seed = setSeed)
val trainingDf = splits(0).cache()
val testDf = splits(1)

val formula = new RFormula().setFormula(rFormula).setFeaturesCol(featureVec).setLabelCol(labelCol)
val dm = formula.fit(trainingDf).transform(trainingDf).select(labelCol, featureVec)
val rD = dm.rdd.map(row => LabeledPoint(row.getAs[Double](labelCol), OldVectors.fromML(row.getAs[DenseVector](featureVec))))

val svmAlg = new SVMWithSGD()
svmAlg.optimizer.setNumIterations(maxIter).setRegParam(regParam).setUpdater(new L1Updater)
val modelL1 = svmAlg.run(rD)

// Weights
val attrs = AttributeGroup.fromStructField(dm.schema(1))
val paramNames = attrs.attributes.get.map(_.name.get)
val paramVals = modelL1.weights.toArray
val paramDf = spark.createDataFrame(paramNames zip paramVals).toDF("Parameter","Weights").as("Weights")
staResults += paramDf

// Test evaluation
val dm1 = formula.fit(testDf).transform(testDf).select(labelCol, featureVec)
val rD1 = dm1.rdd.map(row => LabeledPoint(row.getAs[Double](labelCol), OldVectors.fromML(row.getAs[DenseVector](featureVec))))
val scoreAndLabels = rD1.map { point =>
  val score = modelL1.predict(point.features)
  (score, point.label)
}
val metrics = new BinaryClassificationMetrics(scoreAndLabels)
val auROC = metrics.areaUnderROC()

val gofDf = spark.createDataFrame(Array("ROC: Area under curve") zip Array(auROC)).toDF("Statistic","Value").as("Goodness of Fit(test set)")
staResults += gofDf
val scoreDf = spark.createDataFrame(scoreAndLabels).toDF("score","label").as("Scores (test set)")
staResults += scoreDf
```

---

## Ensemble / Pipeline Nodes

### Spark Random Forest Pipeline (Feature Selection via RF)

**NodeName:** `Spark Random Forest Pipeline`\
**ShortDescription:** Feature selection using Random Forest feature importance within ML Pipeline. Handles mixed continuous/categorical predictors.\
**RequiresInput:** `True`\
**HasOutputDataset:** `False`\
**MaxNumberOfInputs:** 1\
**Output:** `staResults` with "Predictor Importance" DataFrame for selected features

#### Parameters
| Name | Type | DisplayName | Category | Required | Default | Constraints |
|------|------|-------------|----------|----------|---------|-------------|
| `depVar` | String (VarSel) | Continuous dependent | Variable Selection | No | - | VariableType=2, Min=1, Max=1 |
| `_vsConPreds` | String (VarSel) | Continuous predictors | Variable Selection | No | - | VariableType=2, Min=0 |
| `_vsCatPreds` | String (VarSel) | Categorical predictors | Variable Selection | No | - | VariableType=1, Min=0 |
| `numBuckets` | Int | Number of buckets | Discretizer Parameters | Yes | 10 | Min=2 |
| `maxBins` | Int | Number of bins | Random Forest Parameters | Yes | 10 | Min=2 |
| `numTrees` | Int | Number of trees | Random Forest Parameters | Yes | 200 | Min=1 |
| `maxDepth` | Int | Number of levels | Random Forest Parameters | Yes | 2 | Min=0 |
| `impurityMeasure` | Enum | Impurity measure | Random Forest Parameters | No | `gini` | gini, entropy |
| `relError` | Double | Relative error | Discretizer Parameters | Yes | 0.005 | Min=0, Max=1 |

#### Scala Code
```scala
//
// Minimum Spark Version: 2.0
// ML Pipeline: QuantileDiscretizer (depVar) + StringIndexer (cat) + VectorAssembler + RandomForestClassifier
// Output: feature importance for all input predictors
//
import org.apache.spark.ml.classification.{RandomForestClassificationModel, RandomForestClassifier}
import org.apache.spark.ml.feature.{VectorAssembler, StringIndexer, QuantileDiscretizer}
import org.apache.spark.ml.Pipeline
import org.apache.spark.rdd.RDD
import org.apache.spark.sql._
import scala.collection.breakOut

println("continuos vars:" + _vsConPreds)
println("catvars:" + _vsCatPreds)
if (_vsConPreds.isEmpty && _vsCatPreds.isEmpty) throw new Exception("Empty variable selection")

val conPred = _vsConPreds(0) match { case "" => null; case _ => _vsConPreds.toArray }
val catPred = _vsCatPreds(0) match { case "" => null; case _ => _vsCatPreds.toArray }
val inputDataFrame:DataFrame = staIn(0)

val labelVar = "label"
val idxLabel = "indexedLabel"
val featureVec = "features"
val catFeature = "catFeatureIdx"
val selFeatures = "selectedFeatures"
var mapImportance: Map[String, Double] = null

if (conPred != null && catPred != null) {
  val catIndexed = catPred.map(c => s"${c}_idx")
  val inVars = conPred ++ catIndexed
  val discretizer = new QuantileDiscretizer().setInputCol(depVar).setOutputCol(labelVar).setNumBuckets(numBuckets).setRelativeError(relError)
  val labelIndexer = new StringIndexer().setInputCol(labelVar).setOutputCol(idxLabel)
  val indexers = catPred.map(c => new StringIndexer().setInputCol(c).setOutputCol(s"${c}_idx").fit(inputDataFrame))
  val assembler = new VectorAssembler().setInputCols(inVars).setOutputCol(featureVec)
  val tm = new RandomForestClassifier().setLabelCol(idxLabel).setFeaturesCol(featureVec).setNumTrees(numTrees).setImpurity(impurityMeasure).setMaxDepth(maxDepth).setMaxBins(maxBins)
  val pipeline = new Pipeline().setStages(Array(discretizer, labelIndexer) ++ indexers ++ Array(assembler, tm))
  val model = pipeline.fit(inputDataFrame)
  val nStages = model.stages.size - 1
  val importanceVec = model.stages(nStages).asInstanceOf[RandomForestClassificationModel].featureImportances.toSparse
  val inVars1 = conPred ++ catPred
  val selVars = importanceVec.indices.map(i => inVars1(i))
  mapImportance = (selVars zip importanceVec.values)(breakOut): Map[String,Double]
} else if (conPred != null) {
  val discretizer = new QuantileDiscretizer().setInputCol(depVar).setOutputCol(labelVar).setNumBuckets(numBuckets).setRelativeError(relError)
  val labelIndexer = new StringIndexer().setInputCol(labelVar).setOutputCol(idxLabel)
  val assembler = new VectorAssembler().setInputCols(conPred).setOutputCol(featureVec)
  val tm = new RandomForestClassifier().setLabelCol(idxLabel).setFeaturesCol(featureVec).setNumTrees(numTrees).setImpurity(impurityMeasure).setMaxDepth(maxDepth).setMaxBins(maxBins)
  val pipeline = new Pipeline().setStages(Array(discretizer, labelIndexer, assembler, tm))
  val model = pipeline.fit(inputDataFrame)
  val nStages = model.stages.size - 1
  val importanceVec = model.stages(nStages).asInstanceOf[RandomForestClassificationModel].featureImportances.toSparse
  val selVars = importanceVec.indices.map(i => conPred(i))
  mapImportance = (selVars zip importanceVec.values)(breakOut): Map[String,Double]
} else if (catPred != null) {
  val catIndexed = catPred.map(c => s"${c}_idx")
  val discretizer = new QuantileDiscretizer().setInputCol(depVar).setOutputCol(labelVar).setNumBuckets(numBuckets).setRelativeError(relError)
  val labelIndexer = new StringIndexer().setInputCol(labelVar).setOutputCol(idxLabel)
  val indexers = catPred.map(c => new StringIndexer().setInputCol(c).setOutputCol(s"${c}_idx").fit(inputDataFrame))
  val assembler = new VectorAssembler().setInputCols(catIndexed).setOutputCol(featureVec)
  val tm = new RandomForestClassifier().setLabelCol(idxLabel).setFeaturesCol(featureVec).setNumTrees(numTrees).setImpurity(impurityMeasure).setMaxDepth(maxDepth).setMaxBins(maxBins)
  val pipeline = new Pipeline().setStages(Array(discretizer, labelIndexer) ++ indexers ++ Array(assembler, tm))
  val model = pipeline.fit(inputDataFrame)
  val nStages = model.stages.size - 1
  val importanceVec = model.stages(nStages).asInstanceOf[RandomForestClassificationModel].featureImportances.toSparse
  val selVars = importanceVec.indices.map(i => catPred(i))
  mapImportance = (selVars zip importanceVec.values)(breakOut): Map[String,Double]
}

val imDf = spark.createDataFrame(Array(mapImportance.toSeq.sortWith(_._2 > _._2):_*)).toDF("Parameter","Relative Importance").as("Predictor Importance")
staResults += imDf
```

---

## Template Node

### Spark Scala Script (Empty Template)

**NodeName:** `Spark Scala Script`\
**ShortDescription:** Starter template for custom Spark Scala nodes.\
**RequiresInput:** `False`\
**HasOutputDataset:** `True`\
**MaxNumberOfInputs:** 5\
**Parameters:** None (empty PropertyCollection)\
**Code:** Empty (CodeModified=false - reads default from engine)\

#### Metadata (Standard)
- `DataInToken`: `staIn`
- `DataOutToken`: `staOut`
- `ResultsToken`: `staResults`
- `Version`: `13.3`

#### Spark Options (Defaults differ slightly)
- `Downstream:GetData`: `false` (vs `true` for other nodes)

---

## Common Parameter Patterns

### Variable Selection Parameters (Recurring Pattern)
```xml
<Property Name="depVar" Type="System.String" Convertor="None"
    Editor="PropertyGridHelper.VariableSelectionEditor, PropertyGridHelper, Version=13.0.0.0, Culture=neutral, PublicKeyToken=null|..."
    DisplayName="..." Category="Variable Selection" ...>
  <CustomMetaData Count="6">
    <MetaData Key="GCNDisplayType">VariableSelection</MetaData>
    <MetaData Key="VariableType">2</MetaData>  <!-- 0=Binary, 1=Categorical, 2=Continuous -->
    <MetaData Key="MinVars">1</MetaData>
    <MetaData Key="MaxVars">1</MetaData>
    <MetaData Key="VariableSelectionTitle">Variable Selection Dialog</MetaData>
    <MetaData Key="VariableSelectionCaption">Please select desired variables:</MetaData>
  </CustomMetaData>
</Property>

<Property Name="_vsConPreds" Type="System.String" ...>
  <CustomMetaData Count="5">
    <MetaData Key="GCNDisplayType">VariableSelection</MetaData>
    <MetaData Key="VariableType">2</MetaData>
    <MetaData Key="MinVars">0</MetaData>  <!-- or 1 for required -->
    <MetaData Key="VariableSelectionTitle">Variable Selection Dialog</MetaData>
    <MetaData Key="VariableSelectionCaption">Please select desired variables:</MetaData>
  </CustomMetaData>
</Property>

<Property Name="_vsCatPreds" Type="System.String" ...>
  <CustomMetaData Count="5">
    <MetaData Key="GCNDisplayType">VariableSelection</MetaData>
    <MetaData Key="VariableType">1</MetaData>
    <MetaData Key="MinVars">0</MetaData>
    ...
  </CustomMetaData>
</Property>
```

### Numeric Parameters with Constraints
```xml
<Property Name="paramName" Type="System.Int32" ...>
  <CustomMetaData Count="1">
    <MetaData Key="MinValue">0</MetaData>
  </CustomMetaData>
  <Value IsNull="False">default</Value>
</Property>

<Property Name="paramName" Type="System.Double" ...>
  <CustomMetaData Count="2">
    <MetaData Key="MinValue">0</MetaData>
    <MetaData Key="MaxValue">1</MetaData>
  </CustomMetaData>
</Property>
```

### Enum Parameters
```xml
<Property Name="paramName" Type="CustomType" ...
    Editor="PropertyGridHelper.EnumValueEditor, PropertyGridHelper, Version=13.0.0.0, Culture=neutral, PublicKeyToken=null|...">
  <CustomType BaseType="Enum" Count="N">
    <Label Value="0">option1</Label>
    <Label Value="1">option2</Label>
    ...
  </CustomType>
  <Value IsNull="False">defaultOption</Value>
</Property>
```

### Save Model Parameters (Tree/Forest Models)
```xml
<Property Name="saveModel" Type="System.Boolean" ... DisplayName="Save model" Category="Save Model" ...>
  <Value IsNull="False">true</Value>
</Property>
<Property Name="savePath" Type="System.String" ... DisplayName="HDFS path to save to" Category="Save Model" ...>
  <Value IsNull="False">hdfs:///user/statistica</Value>
</Property>
<Property Name="saveFile" Type="System.String" ... DisplayName="Model name" Category="Save Model" ...>
  <Value IsNull="False">treeModel</Value>
</Property>
```

---

## Common Code Patterns

### 1. Standard Imports
```scala
import org.apache.spark.sql._
import org.apache.spark.ml.Pipeline
import org.apache.spark.ml.feature.{VectorAssembler, StringIndexer, RFormula, QuantileDiscretizer, OneHotEncoder, IndexToString, ChiSqSelector}
import org.apache.spark.ml.attribute.AttributeGroup
import org.apache.spark.ml.classification.{...}
import org.apache.spark.ml.regression.{...}
import org.apache.spark.ml.evaluation.{MulticlassClassificationEvaluator, RegressionEvaluator, BinaryClassificationMetrics}
import org.apache.spark.mllib.stat.Statistics
import org.apache.spark.mllib.regression.LabeledPoint
import org.apache.spark.mllib.linalg.{Vectors => OldVectors}
import org.apache.spark.ml.linalg.DenseVector
import org.apache.spark.rdd.RDD
import scala.collection.breakOut
```

### 2. Input DataFrame Access
```scala
val inputDataFrame: DataFrame = staIn(0)  // Single input
// For multi-input nodes: staIn(0), staIn(1), ...
```

### 3. Variable Selection Handling
```scala
val conPred = _vsConPreds.toArray
val catPred = _vsCatPreds.toArray
// Or with null checks:
val conPred = _vsConPreds(0) match { case "" => null; case _ => _vsConPreds.toArray }
```

### 4. RFormula for Feature Assembly (Regression/Classification)
```scala
val labelCol = "label"
val featureVec = "features"
var rFormula = depVar + " ~ " + conPred.mkString("+")
if (!setIntercept) rFormula = rFormula + " - 1"

val formula = new RFormula().setFormula(rFormula).setFeaturesCol(featureVec).setLabelCol(labelCol)
val dm = formula.fit(inputDataFrame).transform(inputDataFrame).select(labelCol, featureVec)
// With weight: .select(labelCol, featureVec, weightCol)
```

### 5. Train/Test Split
```scala
val trainPct = 1.0 - setTestPct  // or testPct
val Array(trainingData, testData) = inputDataFrame.randomSplit(Array(trainPct, setTestPct), seed = setSeed)
```

### 6. Pipeline Construction Pattern
```scala
val stages = Array(stage1, stage2, ..., estimator)
val pipeline = new Pipeline().setStages(stages)
val model = pipeline.fit(trainingData)
val predictions = model.transform(testData)
```

### 7. Feature Importance Extraction (Tree-based Models)
```scala
val nStages = model.stages.size - 1  // or -2 for classifier with label converter
val importanceVec = model.stages(nStages).asInstanceOf[ModelType].featureImportances.toSparse
val selVars = importanceVec.indices.map(i => conPred(i))
val mapImportance = (selVars zip importanceVec.values)(breakOut): Map[String,Double]
val imDf = spark.createDataFrame(Array(mapImportance.toSeq.sortWith(_._2 > _._2):_*)).toDF("Parameter","Relative Importance").as("Predictor Importance")
staResults += imDf
```

### 8. Coefficient Extraction (Linear Models)
```scala
val attrs = AttributeGroup.fromStructField(dm.schema(1))
val pN = attrs.attributes.get.map(_.name.get)
val paramNames = Array("Intercept") ++ pN  // or pN ++ Array("Intercept") for GLM
val paramVals = Array(model.intercept) ++ model.coefficients.toArray
val paramDf = spark.createDataFrame(paramNames zip paramVals).toDF("Parameter","Estimate").as("Parameter estimates")
staResults += paramDf
```

### 9. GLM Coefficient Statistics
```scala
val paramMap = paramNames.zip(paramVals).zip(summary.coefficientStandardErrors).zip(summary.tValues).zip(summary.pValues).map{ case ((((a, b), c), d), e) => (a,b,c,d,e) }
val paramDf = spark.createDataFrame(paramMap).toDF("Parameter","Estimate","Standard Error","t","p").as("Parameter estimates")
```

### 10. Model Saving (Tree/Forest)
```scala
if (saveModel) {
  val saveStr = savePath + "/" + saveFile
  model.stages(nStages).asInstanceOf[ModelType].save(saveStr)
}
```

### 11. Evaluation Metrics
```scala
// Regression
val evaluator = new RegressionEvaluator().setLabelCol(depVar).setPredictionCol("prediction").setMetricName("rmse")
val rmse = evaluator.evaluate(predictions)

// Classification (Multiclass)
val evaluator = new MulticlassClassificationEvaluator().setLabelCol(labelCol).setPredictionCol(predCol).setMetricName("accuracy")
val accuracy = evaluator.evaluate(predictions)

// Classification (Binary - SVM uses MLlib)
val metrics = new BinaryClassificationMetrics(scoreAndLabels)
val auROC = metrics.areaUnderROC()
```

### 12. Results Output to staResults
```scala
// Single DataFrame
staResults += df.as("Alias Name")

// Multiple DataFrames
staResults += df1.as("Name1")
staResults += df2.as("Name2")
```

### 13. Categorical Preprocessing Pipeline (Mixed Types)
```scala
// For continuous: QuantileDiscretizer
val discretizers = (Array(depVar) ++ conPred).map(c => new QuantileDiscretizer().setInputCol(c).setOutputCol(s"${c}_disc").setNumBuckets(numBuckets).setRelativeError(relError))

// For categorical: StringIndexer
val indexers = catPred.map(c => new StringIndexer().setInputCol(c).setOutputCol(s"${c}_idx"))

// Assemble
val assembler = new VectorAssembler().setInputCols(indexedCols ++ discretizedCols).setOutputCol("features")
```

### 14. Error Handling Pattern
```scala
if (_vsConPreds.isEmpty && _vsCatPreds.isEmpty) throw new Exception("Empty variable selection")
if (dataFrame.select(col).count > 0) { ... } else { 0 }  // Handle empty partitions
```

---

## Node Categorization Summary

| Category | Nodes | Input | Output | Primary Output |
|----------|-------|-------|--------|----------------|
| **Data I/O** | Spark Data | 0 | DataFrame | `staOut` |
| **Preprocessing** | Make Design Matrix, Index Categoricals, Reference Code Categoricals | 1 | DataFrame | `staOut` |
| **Feature Selection** | Feature Selection, Feature Selection Pipeline | 1 | Results | `staResults` |
| **Regression** | Linear Regression, GLM, Decision Tree Regressor, Random Forest Regressor | 1 | Results | `staResults` (+ model save) |
| **Classification** | Logistic Regression, Decision Tree Classifier, Random Forest Classifier, SVM | 1 | Results | `staResults` (+ model save) |
| **Pipeline/Ensemble** | Random Forest Pipeline | 1 | Results | `staResults` |
| **Template** | Spark Scala Script | 0-5 | DataFrame | `staOut` |

---

## Implementation Notes for Node Editor

1. **Node Ports**:
   - Input port(s): Connect to `staIn` (array of DataFrames)
   - Output port: `staOut` (DataFrame) for transformers
   - Results port: `staResults` (array of DataFrames) for models/analysis

2. **Parameter UI**: Use the `VariableSelectionEditor` for column selectors with proper `VariableType` metadata

3. **Code Execution**: Scala code runs in Spark cluster via Livy; `spark` session is implicit

4. **Results Display**: Each `staResults += df.as("Name")` creates a named result table

5. **Model Persistence**: Save path uses HDFS; editor should validate HDFS connectivity

6. **Spark Version**: All nodes require Spark 2.0+ (uses ML Pipeline API)

7. **Missing Data**: Most nodes assume no missing values; `Spark Data` has `naDrop` option

8. **Categorical Handling**: Two approaches - Indexing (`x_idx`) or One-Hot (`x_idx_vec`); Design Matrix uses RFormula which handles both

---
