# Parquet source: переносимый тест

## §1. Задача

Проверить чтение локального Parquet и сохранение схемы в Spark worker.
Запись Parquet через Spark не является поддерживаемой нодой этой версии.

## §2. Алгоритм

В уникальном временном каталоге создаётся один Parquet-файл через
ExampleParquetWriter и LocalOutputFile (Java NIO). Worker выполняет источник,
Filter, Select Columns, Limit и Preview. Проверяются значения и схема,
затем временное дерево удаляется независимо от исхода теста.

## §3. Применение

Один тест WorkerSuite исполняется на Windows/Linux/macOS без требования
winutils для подготовки входного файла. Это не гарантия, что все операции
Hadoop/Spark на Windows обходятся без native dependencies.

## §4. Контракт

Fixture: quantity — int32 1..12, amount — int32 2*quantity+3, region — UTF8 EU.
Фильтр quantity >= 4, projection quantity/amount, limit 3.
Оракул: строки (4,11), (5,13), (6,15); Spark-типы обеих колонок integer.
Численные допуски не нужны: сравниваются малые целые числа точно.

## §5. Реализация

`workers/spark/src/test/scala/datacraft/WorkerSuite.scala`, тест
`Parquet source executes with schema preserved`. Производственный worker
и остальные тесты не меняются. Новые зависимости не добавляются.

## §6. Причина изменения

В исходном пользовательском отчёте TEST-datacraft.WorkerSuite.xml пять тестов
прошли. Этот тест упал на data.write.parquet при создании fixture:
HADOOP_HOME и hadoop.home.dir отсутствуют; RawLocalFileSystem.setPermission
обращается к Shell.getWinUtilsPath. Чтение Parquet ещё не выполнялось.
Исправление отделяет подготовку fixture от неподдерживаемого Spark export.

verified 2026-11-08 via локальный classpath sbt: Parquet 1.15.2 содержит
ExampleParquetWriter и LocalOutputFile. Конструктор OutputFile builder проверен
по [исходнику Apache](https://github.com/apache/parquet-java/blob/apache-parquet-1.15.2/parquet-hadoop/src/main/java/org/apache/parquet/hadoop/example/ExampleParquetWriter.java).

## §7. Проверка

Из workers/spark: `sbt test writeClasspath`. Результат повторного прогона
исправленного теста нужно отличать от исходного отчёта 5 PASS / 1 FAIL.

verified 2026-11-08 via локальный запуск: исправленный WorkerSuite.scala
скомпилирован scala.tools.nsc.Main 2.13.16 с classpath, экспортированным sbt.
JUnitCore выполнил WorkerSuite: OK (6 tests), exit 0, Spark 4.0.1,
Temurin 21.0.13, Windows 11. Логи: .tools/parquet-test-check/compile.log и
.tools/parquet-test-check/test.log. Командные argument files сохранены рядом.
Штатная команда sbt test writeClasspath после изменения в агентской сессии
не запускалась; создание classpath.txt этим прогоном не проверено.

## §8′. Устойчивость

Файл создаётся только внутри уникального временного каталога теста; writer
закрывается в finally. Удаление ограничено этим деревом. Исключения подготовки
и чтения не превращаются в SKIP; тест должен завершаться ошибкой при дефекте.
Java NIO fixture не проверяет способность Spark записывать Parquet на Windows.
