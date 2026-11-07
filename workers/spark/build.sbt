ThisBuild / scalaVersion := "2.13.16"
ThisBuild / version := "0.1.0"
ThisBuild / organization := "org.datacraft"

lazy val writeClasspath = taskKey[File]("Write absolute runtime classpath for the Rust client")

lazy val root = (project in file("."))
  .settings(
    name := "datacraft-spark-worker",
    libraryDependencies ++= Seq(
      "org.apache.spark" %% "spark-sql" % "4.0.1",
      "org.apache.spark" %% "spark-mllib" % "4.0.1",
      "com.lihaoyi" %% "ujson" % "4.2.1",
      "org.scalameta" %% "munit" % "1.0.4" % Test
    ),
    Test / fork := true,
    Test / parallelExecution := false,
    Test / javaOptions ++= Seq(
      "-Xmx2g",
      "--add-opens=java.base/sun.nio.ch=ALL-UNNAMED",
      "--add-opens=java.base/java.nio=ALL-UNNAMED",
      "--add-opens=java.base/java.lang.invoke=ALL-UNNAMED",
      "--add-opens=java.base/java.util=ALL-UNNAMED"
    ),
    writeClasspath := {
      val entries = (Compile / fullClasspath).value.files
      val output = target.value / "classpath.txt"
      IO.write(output, entries.map(_.getAbsolutePath).mkString(java.io.File.pathSeparator))
      output
    }
  )
