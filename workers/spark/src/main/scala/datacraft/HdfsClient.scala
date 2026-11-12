package datacraft

import java.net.URI
import java.security.PrivilegedExceptionAction
import java.util.UUID
import org.apache.hadoop.conf.Configuration
import org.apache.hadoop.fs.{FileSystem, Path}
import org.apache.hadoop.security.UserGroupInformation
import ujson.{Value,Obj,Arr}

/** Shared real WebHDFS client; redirects/HTTP authentication are Hadoop's responsibility. */
object HdfsClient {
  /** Publish only a fully closed file; never delete the destination before commit. */
  def publish(fs: FileSystem, target: Path, overwrite: Boolean, createParents: Boolean)(write: Path => Unit): Unit = {
    require(target.getName.nonEmpty,"Cannot export to HDFS root")
    require(!fs.exists(target) || (overwrite && fs.getFileStatus(target).isFile),"Destination exists or is not a file; enable overwrite explicitly")
    val parent=target.getParent
    if(!fs.exists(parent)) {
      require(createParents,"Parent directory does not exist")
      require(fs.mkdirs(parent),"Could not create HDFS parent directories")
    }
    val temporary=new Path(parent,"."+target.getName+".datacraft-"+UUID.randomUUID().toString+".tmp")
    var committed=false
    try {
      write(temporary)
      val option=if(overwrite) org.apache.hadoop.fs.Options.Rename.OVERWRITE else org.apache.hadoop.fs.Options.Rename.NONE
      fs.asInstanceOf[org.apache.hadoop.hdfs.web.WebHdfsFileSystem].rename(temporary,target,option)
      committed=true
    } finally {
      if(!committed) try fs.delete(temporary,false) catch {case scala.util.control.NonFatal(_)=>System.err.println("DataCraft: temporary HDFS export file retained after failure")}
    }
  }
  def list(config: Value, directory: String): Value = withClient(config) {fs=>
    val target=path(directory)
    val iterator=fs.listStatusIterator(target)
    val rows=scala.collection.mutable.Buffer.empty[Value]
    while(iterator.hasNext && rows.size<1000) {
      val entry=iterator.next()
      rows+=Obj("path"->entry.getPath.toUri.getPath,"directory"->entry.isDirectory,"length"->entry.getLen.toString)
    }
    Obj("name"->"HDFS directory (first 1000 entries)","schema"->Obj("type"->"struct","fields"->Arr(
      Obj("name"->"path","type"->"string","nullable"->false,"metadata"->Obj()),
      Obj("name"->"directory","type"->"boolean","nullable"->false,"metadata"->Obj()),
      Obj("name"->"length","type"->"string","nullable"->false,"metadata"->Obj()))),
      "rows"->Arr.from(rows))
  }
  def withClient[A](config: Value)(action: FileSystem => A): A = {
    val protocol = config("protocol").str
    require(Set("webhdfs", "swebhdfs").contains(protocol), "Unsupported HDFS protocol")
    val host = config("host").str
    require(host.nonEmpty && host.forall(c => c.isLetterOrDigit || ".-:[]".contains(c)), "Invalid HDFS host")
    val port = config("port").num.toInt
    require(port >= 1 && port <= 65535, "Invalid HDFS port")
    val timeout = config("timeout_seconds").num.toInt
    require(timeout >= 1 && timeout <= 3600, "Invalid HDFS timeout")
    val conf = new Configuration(false)
    conf.set("fs.webhdfs.impl", "org.apache.hadoop.hdfs.web.WebHdfsFileSystem")
    conf.set("fs.swebhdfs.impl", "org.apache.hadoop.hdfs.web.SWebHdfsFileSystem")
    conf.setInt("dfs.webhdfs.socket.connect-timeout", timeout * 1000)
    conf.setInt("dfs.webhdfs.socket.read-timeout", timeout * 1000)
    conf.setInt("dfs.http.client.retry.max.attempts", 0)
    conf.setBoolean("dfs.client.use.datanode.hostname", true)
    val uri = new URI(protocol, null, host, port, "/", null, null)
    def run(): A = {
      val fs = FileSystem.newInstance(uri, conf)
      try action(fs) finally fs.close()
    }
    config("authentication").str match {
      case "none" => run()
      case "simple" =>
        val user = config("username").str
        require(user.trim.nonEmpty, "Simple authentication requires a username")
        try UserGroupInformation.createRemoteUser(user).doAs(new PrivilegedExceptionAction[A] {override def run(): A = HdfsClient.open(uri, conf, action)})
        catch {case error: java.lang.reflect.UndeclaredThrowableException if error.getCause!=null => throw error.getCause}
      case _ => throw new IllegalArgumentException("Only none and Simple authentication are supported")
    }
  }
  private def open[A](uri: URI, conf: Configuration, action: FileSystem => A): A = {
    val fs = FileSystem.newInstance(uri, conf)
    try action(fs) finally fs.close()
  }
  def path(value: String): Path = {
    require(value.startsWith("/") && !value.exists(c => c == '\u0000' || "?#\\".contains(c))
      && !value.split('/').exists(p => p == "." || p == ".."), "Expected an absolute HDFS path")
    new Path(value)
  }
}
