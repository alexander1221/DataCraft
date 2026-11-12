package datacraft

import java.io.{BufferedReader, BufferedWriter, InputStreamReader, OutputStreamWriter}
import java.nio.charset.{Charset, CodingErrorAction, StandardCharsets}
import java.nio.file.{Files, Path => LocalPath}
import org.apache.spark.sql.{DataFrame, SparkSession}
import org.apache.spark.sql.types._
import ujson.Value
import scala.util.control.NonFatal
import scala.util.Using

/** Streaming transport/format integration; semantics and limits: docs/HDFS_TEXT.md. */
object HdfsText {
  private def charset(config: Value): Charset = {
    val name=config("encoding").str
    require(Set("UTF-8","windows-1251","windows-1252","US-ASCII","UTF-16LE","UTF-16BE").contains(name),"Unsupported encoding")
    Charset.forName(name)
  }
  private def canonical(out: java.io.Writer, values: Seq[String]): Unit = {
    out.write(values.map(v=>"\""+v.replace("\"","\"\"")+"\"").mkString(",")); out.write("\n")
  }
  private def number(value: String, decimal: String): String = {
    if (decimal == "," && value.matches("[-+]?[0-9]+(,[0-9]+)?([eE][-+]?[0-9]+)?")) value.replace(',','.') else value
  }
  final case class Imported(table: DataFrame, spool: LocalPath)
  def read(spark: SparkSession, config: Value): Imported = HdfsClient.withClient(config) { fs =>
    val path=HdfsClient.path(config("hdfs_path").str)
    require(fs.getFileStatus(path).isFile,"HDFS import requires a file, not a directory")
    val local=Files.createTempFile("datacraft-hdfs-", ".csv")
    var success=false
    try {
      var names=Vector.empty[String]
      var width=0
      var skipped=0L
      var imported=0L
      val options=ujson.read(config("column_options").str).arr.map(v=>v("column").num.toInt-1 -> v).toMap
      val types=scala.collection.mutable.Map.empty[Int,String]
      var selected=Vector.empty[Int]
      var done=false
      val format=new DelimitedText.Format(config)
      val decoder=charset(config).newDecoder().onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT)
      Using.Manager {use=>
        val stream=use(fs.open(path))
        val input=use(new BufferedReader(new InputStreamReader(stream,decoder)))
        val parser=use(new DelimitedText.Parser(input,format))
        val output=use(Files.newBufferedWriter(local,StandardCharsets.UTF_8))
        while (!done) {
          parser.next() match {
            case None => done=true
            case Some(record) if config("skip_blank_lines").bool && record.blank => ()
            case Some(_) if skipped < config("skip_rows").num.toLong => skipped+=1
            case Some(record) =>
              val firstRecord=width==0
              if (width==0) {
                width=record.fields.size
                require(options.keys.forall(i=>i>=0 && i<width),"Column option references an absent column")
                selected=(0 until width).filterNot(i=>options.get(i).exists(_.obj.get("omit").exists(_.bool))).toVector
                require(selected.nonEmpty,"At least one imported column must remain")
                val raw=if(config("header").bool) record.fields else (1 to width).map(i=>s"column_$i").toVector
                names=selected.map {i=> options.get(i).flatMap(_.obj.get("name")).map(_.str).filter(_.nonEmpty).getOrElse(raw(i)).stripPrefix("\uFEFF")}
                require(names.forall(_.trim.nonEmpty) && names.distinct.size==names.size,"Column names must be nonempty and unique")
                selected.zipWithIndex.foreach {case (i, j)=> types(j)=options.get(i).flatMap(_.obj.get("type")).map(_.str).getOrElse("auto")}
                canonical(output,names)
              } else require(record.fields.size==width,s"Record has ${record.fields.size} fields; expected $width")
              if (!(firstRecord && config("header").bool)) {
                val values=selected.map {i=>
                  val option=options.get(i)
                  var value=record.fields(i)
                  if (value==config("null_value").str) value=option.flatMap(_.obj.get("missing_value")).map(_.str).getOrElse(value)
                  if (value!=config("null_value").str && option.exists(_.obj.get("remove_non_numeric").exists(_.bool)))
                    value=value.replaceAll("[^0-9eE+.,-]","").replaceAll("(?<![0-9])[eE]|[eE](?![+-]?[0-9])","")
                  option.flatMap(_.obj.get("text_length")).foreach {n=>require(n.num==0 || value.length<=n.num,s"Column ${i+1} exceeds text length")}
                  val dataType=option.flatMap(_.obj.get("type")).map(_.str).getOrElse("auto")
                  if (dataType=="date" && value!=config("null_value").str) {
                    val pattern=option.flatMap(_.obj.get("input_format")).map(_.str).filter(_.nonEmpty).getOrElse("uuuu-MM-dd")
                    value=java.time.LocalDate.parse(value,java.time.format.DateTimeFormatter.ofPattern(pattern).withResolverStyle(java.time.format.ResolverStyle.STRICT)).toString
                  }
                  if (dataType=="string" || (dataType=="auto" && !config("infer_schema").bool)) value else number(value,config("decimal_separator").str)
                }
                canonical(output,values); imported+=1
                val limit=config("max_rows").num.toLong
                if(limit>0 && imported>=limit) done=true
              }
          }
        }
      }.get
      require(width>0,"No header or data records found in HDFS file")
      // Read explicit string columns first; configured casts are validated by Spark actions.
      val schema=StructType(names.map(n=>StructField(n,StringType,true)))
      var df=spark.read.schema(schema).option("header",true).option("multiLine",true)
        .option("lineSep","\n")
        .option("quote","\"").option("escape","\"").option("nullValue",config("null_value").str)
        .option("mode","FAILFAST").csv(local.toString)
      if(config("infer_schema").bool && types.values.exists(_=="auto")) {
        val inferred=spark.read.option("header",true).option("multiLine",true).option("inferSchema",true)
          .option("lineSep","\n")
          .option("quote","\"").option("escape","\"").option("nullValue",config("null_value").str).csv(local.toString).schema
        names.indices.filter(i=>types(i)=="auto").foreach(i=>df=df.withColumn(names(i),df.col(quoted(names(i))).cast(inferred(i).dataType)))
      }
      names.indices.foreach {i=>
        val dataType=types(i) match {
          case "auto" => None; case "string" => Some(StringType); case "long" => Some(LongType)
          case "double" => Some(DoubleType); case "boolean" => Some(BooleanType); case "date" => Some(DateType)
          case other => throw new IllegalArgumentException(s"Unsupported column type: $other")
        }
        dataType.foreach(t=>df=df.withColumn(names(i),df.col(quoted(names(i))).cast(t)))
      }
      success=true; Imported(df,local)
    } finally {if(!success) Files.deleteIfExists(local)}
  }
  private def quoted(name: String): String = "`"+name.replace("`","``")+"`"

  def write(df: DataFrame, config: Value): Unit = HdfsClient.withClient(config) {fs=>
    df.schema.fields.foreach {f=>
      val scalar=f.dataType.isInstanceOf[NumericType] || Set[DataType](StringType,BooleanType,DateType,TimestampType,TimestampNTZType,NullType).contains(f.dataType)
      require(scalar,s"Export requires scalar columns; unsupported column ${f.name}")
    }
    val target=HdfsClient.path(config("hdfs_path").str)
    val format=new DelimitedText.Format(config)
    val encoder=charset(config).newEncoder().onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT)
    HdfsClient.publish(fs,target,config("overwrite").bool,config("create_parents").bool) {temporary=>
      Using.Manager {use=>
        val stream=use(fs.create(temporary,false))
        val out=use(new BufferedWriter(new OutputStreamWriter(stream,encoder)))
        if(config("header").bool) DelimitedText.writeRecord(out,df.columns.toSeq,format)
        val rows=df.toLocalIterator()
        while(rows.hasNext) {
          val row=rows.next()
          val values=df.schema.fields.indices.map {i=>
            if(row.isNullAt(i)) config("null_value").str else {
              val raw=row.get(i).toString
              if(!config("use_english_numbers").bool && df.schema(i).dataType.isInstanceOf[NumericType])
                raw.replace(".",config("decimal_separator").str) else raw
            }
          }
          DelimitedText.writeRecord(out,values,format)
        }
      }.get
    }
  }
}
