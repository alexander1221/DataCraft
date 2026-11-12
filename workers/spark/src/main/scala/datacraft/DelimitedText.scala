package datacraft

import java.io.{PushbackReader, Reader, Writer}
import ujson.Value

/** Streaming text grammar. No line splitting: quoted records may contain delimiters/newlines. */
object DelimitedText {
  def decode(value: String): String = {
    val out = new StringBuilder
    var i = 0
    while (i < value.length) {
      if (value(i) != '\\') out.append(value(i)) else {
        i += 1
        require(i < value.length, "Incomplete text escape")
        out.append(value(i) match {
          case 'n' => '\n'; case 'r' => '\r'; case 't' => '\t'
          case '\\' => '\\'; case '"' => '"'
          case _ => throw new IllegalArgumentException("Unsupported text escape")
        })
      }
      i += 1
    }
    out.toString
  }
  final class Format(config: Value) {
    private def tokens(name: String, all: String): Vector[String] = {
      val text = decode(config(name).str)
      require(text.nonEmpty && text.length <= 16, s"Invalid $name")
      if (config(all).bool) Vector(text) else text.map(_.toString).distinct.toVector
    }
    val fields = tokens("delimiter", "delimiter_match_all")
    val records = tokens("record_separator", "record_match_all")
    val start = decode(config("quote_start").str)
    val end = decode(config("quote_end").str)
    val escapes = Vector(
      decode(config("record_escape").str) -> records,
      decode(config("delimiter_escape").str) -> fields,
      decode(config("quote_start_escape").str) -> Vector(start),
      decode(config("quote_end_escape").str) -> Vector(end)
    ).filter(_._1.nonEmpty)
    val prefixes = escapes.map(_._1).distinct
    val collapse = config("ignore_consecutive_delimiters").bool
    val maxField = config("max_field_chars").num.toInt
    require(maxField >= 1 && maxField <= 16777216, "Invalid maximum field size")
    require(start.length <= 1 && end.length <= 1 && start.isEmpty == end.isEmpty, "Invalid quoting characters")
    require(escapes.forall(_._1.length == 1), "Escape characters must be single characters")
    require(!fields.exists(f => records.exists(r => f.exists(r.contains(_)))), "Field/row separators overlap")
    val escapedTokens: Vector[(String,String)] = (escapes.flatMap {case (prefix, values) =>
      values.filter(_.nonEmpty).map(v => (prefix+v,v))
    } ++ prefixes.map(p => (p+p,p))).distinct.sortBy {case (token,_) => -token.length}
  }

  final case class Record(fields: Vector[String], blank: Boolean)
  final class Parser(input: Reader, format: Format) extends AutoCloseable {
    private val reader = new PushbackReader(input, 64)
    private val first=reader.read()
    if(first>=0 && first!=0xFEFF) reader.unread(first)
    private var count = 0
    private def consumed(n: Int): Unit = { count += n; require(count <= 16777216, "Text record exceeds 16 Mi characters") }
    private def take(token: String): Boolean = {
      if (token.isEmpty) return false
      val got = new Array[Char](token.length)
      var n = 0
      var eof = false
      while (n < token.length && !eof) {
        val c = reader.read()
        if (c < 0) eof = true else {got(n)=c.toChar; n+=1}
      }
      if (n == token.length && new String(got) == token) {consumed(n); true}
      else {if (n > 0) reader.unread(got,0,n); false}
    }
    private def any(tokens: Vector[String]): Boolean = tokens.exists(take)
    def next(): Option[Record] = {
      count = 0
      val values = Vector.newBuilder[String]
      var field = new StringBuilder
      var quoted = false
      var closed = false
      var content = false
      var hadAnything = false
      var columns = 0
      def append(text: String): Unit = {field.append(text); require(field.length <= format.maxField,"Text field exceeds configured size")}
      def finish(): Unit = {
        columns+=1; require(columns<=10000,"Text record exceeds 10000 fields")
        values += field.toString; field = new StringBuilder; closed=false
      }
      while (true) {
        val opening = !quoted && field.isEmpty && !closed && take(format.start)
        val escaped = if (opening) None else format.escapedTokens.find {case (token,_) => take(token)}
        if (opening) {quoted=true; content=true; hadAnything=true}
        else if (escaped.nonEmpty) {require(!closed,"Unexpected escape after closing quote");append(escaped.get._2); content=true; hadAnything=true}
        else if (quoted) {
          if (take(format.end)) {quoted=false; closed=true; hadAnything=true}
          else {
            val c=reader.read(); require(c>=0,"Unterminated quoted field")
            consumed(1); append(c.toChar.toString); content=true; hadAnything=true
          }
        } else if (any(format.records)) {finish(); return Some(Record(values.result(),!content))}
        else if (any(format.fields)) {
          finish(); content=true; hadAnything=true
          if (format.collapse) while (any(format.fields)) {}
        } else {
          val c=reader.read()
          if (c<0) {
            if (!hadAnything && columns==0 && field.isEmpty) return None
            finish(); return Some(Record(values.result(),!content))
          }
          require(!closed,"Unexpected text after closing quote")
          consumed(1); append(c.toChar.toString); content ||= !c.toChar.isWhitespace; hadAnything=true
        }
      }
      None
    }
    override def close(): Unit = reader.close()
  }

  def writeRecord(writer: Writer, values: Seq[String], format: Format): Unit = {
    require(values.size<=10000,"Export record exceeds 10000 fields")
    var written=0
    def size(n:Int):Unit={written+=n;require(written<=16777216,"Export record exceeds 16 Mi characters")}
    def emit(text:String):Unit={size(text.length);writer.write(text)}
    def character(c:Char):Unit={size(1);writer.write(c.toInt)}
    values.zipWithIndex.foreach {case (value,index) =>
      require(value.length<=format.maxField,"Export field exceeds configured size")
      if (index>0) emit(format.fields.head)
      val special=(format.fields++format.records).exists(value.contains) ||
        (format.start.nonEmpty && (value.contains(format.start) || value.contains(format.end)))
      val quoted=format.start.nonEmpty && (value.isEmpty || special || value.headOption.exists(_.isWhitespace) || value.lastOption.exists(_.isWhitespace))
      if (quoted) emit(format.start)
      var i=0
      while (i<value.length) {
        val target = if (quoted && value.startsWith(format.end,i)) Some(format.end)
          else if (quoted && value.startsWith(format.start,i)) Some(format.start)
          else if (!quoted) (format.fields++format.records).find(value.startsWith(_,i)) else None
        target match {
          case Some(token) =>
            val prefix=format.escapes.find(_._2.contains(token)).map(_._1)
            require(prefix.nonEmpty,s"Text cannot be represented: configure an escape for delimiter/quote $token")
            emit(prefix.get); emit(token); i+=token.length
          case None =>
            val prefix=format.prefixes.find(value.startsWith(_,i))
            if (prefix.nonEmpty) {emit(prefix.get); emit(prefix.get); i+=prefix.get.length}
            else {character(value.charAt(i)); i+=1}
        }
      }
      if (quoted) emit(format.end)
    }
    emit(format.records.head)
  }
}
