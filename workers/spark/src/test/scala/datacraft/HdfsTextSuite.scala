package datacraft

import java.io.{StringReader,StringWriter}
import ujson.{Obj,Value}

class HdfsTextSuite extends munit.FunSuite {
  private def format(overrides: (String,Value)*): DelimitedText.Format = {
    val value=Obj("delimiter"->"\\t","record_separator"->"\\n","delimiter_match_all"->true,
      "record_match_all"->true,"quote_start"->"\"","quote_end"->"\"","record_escape"->"",
      "delimiter_escape"->"","quote_start_escape"->"","quote_end_escape"->"\"",
      "ignore_consecutive_delimiters"->false,"max_field_chars"->1048576)
    overrides.foreach {case(k,v)=>value(k)=v}; new DelimitedText.Format(value)
  }
  private def parse(text: String,f: DelimitedText.Format): Vector[Vector[String]] = {
    val parser=new DelimitedText.Parser(new StringReader(text),f)
    val rows=Vector.newBuilder[Vector[String]]
    try {var row=parser.next();while(row.nonEmpty) {rows+=row.get.fields;row=parser.next()};rows.result()}
    finally parser.close()
  }
  test("independent CSV grammar: BOM, quoted tabs, CRLF inside strings, doubled quotes and trailing empty fields") {
    val input="\uFEFFid\tname\tvalue\n1\t\"a\tb\r\nc\"\t\"say \"\"yes\"\"\"\n2\t\"\"\t\n"
    assertEquals(parse(input,format()),Vector(Vector("id","name","value"),Vector("1","a\tb\r\nc","say \"yes\""),Vector("2","","")))
  }
  test("custom complete separators and character-set separators have distinct contracts") {
    assertEquals(parse("a||b<EOR>c||d<EOR>",format("delimiter"->ujson.Str("||"),"record_separator"->ujson.Str("<EOR>"))),Vector(Vector("a","b"),Vector("c","d")))
    assertEquals(parse("a,;b;c\n",format("delimiter"->ujson.Str(",;"),"delimiter_match_all"->ujson.Bool(false),"ignore_consecutive_delimiters"->ujson.Bool(true))),Vector(Vector("a","b","c")))
  }
  test("writer emits known escaped bytes and round trips multiline Unicode and empty strings") {
    val output=new StringWriter
    val values=Vector("plain","a\tb","a\"b","строка\n🙂","")
    DelimitedText.writeRecord(output,values,format())
    assertEquals(output.toString,"plain\t\"a\tb\"\t\"a\"\"b\"\t\"строка\n🙂\"\t\"\"\n")
    assertEquals(parse(output.toString,format()),Vector(values))
  }
  test("asymmetric quotes and unquoted delimiter escaping round trip") {
    val custom=format("quote_start"->ujson.Str("["),"quote_end"->ujson.Str("]"),"quote_end_escape"->ujson.Str("\\\\"))
    val output=new StringWriter; DelimitedText.writeRecord(output,Seq("a]b\tc"),custom)
    assertEquals(parse(output.toString,custom),Vector(Vector("a]b\tc")))
    val unquoted=format("quote_start"->ujson.Str(""),"quote_end"->ujson.Str(""),"quote_end_escape"->ujson.Str(""),"delimiter_escape"->ujson.Str("\\\\"))
    assertEquals(parse("a\\\tb\tc\n",unquoted),Vector(Vector("a\tb","c")))
  }
  test("malformed quotes, oversized fields and unrepresentable export fail instead of corrupting text") {
    intercept[IllegalArgumentException](parse("\"unfinished",format()))
    intercept[IllegalArgumentException](parse("\"closed\"junk\n",format()))
    intercept[IllegalArgumentException](parse("12345",format("max_field_chars"->ujson.Num(4))))
    intercept[IllegalArgumentException](DelimitedText.writeRecord(new StringWriter,Seq("a\tb"),format("quote_start"->ujson.Str(""),"quote_end"->ujson.Str(""),"quote_end_escape"->ujson.Str(""))))
  }
}
