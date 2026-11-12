# Import HDFS Text и HDFS Export Text

## §1. Задача

Источник выдаёт существующий Table; приёмник принимает Table и не имеет
выходных портов. Проекты и локальные источники сохраняют совместимость.
Spreadsheet с метками/длинными именами в текущей модели отсутствует.

## §2. Алгоритм

Общий Hadoop WebHDFS client обслуживает OPEN/CREATE, stat/list/mkdir/delete/
rename, включая DataNode redirects. Импорт потоково нормализует текст во
временный CSV на локальном диске, затем создаёт Spark DataFrame. Экспорт
передаёт строки через toLocalIterator в один временный HDFS-файл, закрывает
его и публикует rename. Полные наборы не собираются в Rust/driver memory.

## §3. Применения

HDFS Text → Filter → Preview; CSV Source → HDFS Export Text; round-trip
Unicode, quoted delimiters и переводов строк; просмотр HDFS-каталога.
Ошибки сети, прав доступа, формата и существующего назначения возвращаются
с UUID ноды. Export повторяется только по явному запуску.

## §4. Контракт

Подключение хранится в параметрах нод: name, host, port, WebHDFS/SWebHDFS,
none/simple user, timeout. Имеющиеся подключения выбираются из настроек
HDFS-нод проекта и копируются в inspector draft; Apply — ConfigureNode.
Секреты не сохраняются; Kerberos/password/token не имитируются.
Host не содержит URI/userinfo/path; HDFS path абсолютный, без query/fragment.
Формат задаёт поля/строки, literal match либо набор символов, схлопывание
разделителей, header, charset, null marker, quoting/escaping. Импорт также
задаёт skip/limit/blank lines и типизацию/отбор колонок. Ограничения полей
защищают от неограниченного выделения памяти. UI объясняет escape-последовательности.
Параметры сохраняются в .dcraft v1; UUID и порты стабильны.

Запись по умолчанию запрещает перезапись. Публикация использует WebHDFS rename2
с NONE либо OVERWRITE: существующий файл заменяется только явно разрешённым
rename после полного закрытия записи. Отмена JVM может оставить скрытый
`.datacraft-UUID.tmp`; оригинал до публикации не удаляется. Если ответ о commit
потерян, операция могла завершиться на сервере: exactly-once не обещается.

## §5. Реализация

NodeSpec и typed NodeKind задают все параметры; inspector использует те же
definitions, стандартные widgets, presets Tab/Space/Comma/Semicolon и LF/CRLF/CR,
редактор overrides колонок. Copy existing connection копирует семь полей
подключения в draft, без общей изменяемой сущности и без копирования пути.
Apply/Revert и Project Save/Open сохраняют обычную семантику Session.

`data.hdfs.list` — отдельный read-only background job. Browser показывает
до 1000 записей, позволяет навигацию и выбор/ввод пути. Выбор меняет только
inspector buffer; revision/node UUID проверяются перед применением. Просмотр
не заменяет результаты pipeline и не меняет готовность кэша даже при отмене.
Размер файла передаётся строкой, чтобы не терять точность 64-bit длины в JSON.

`HdfsClient` использует Hadoop из имеющегося Spark classpath. `DelimitedText`
обрабатывает quoted multiline records, asymmetric quotes и escapes без split
по строкам. `HdfsText` отвечает за дисковую нормализацию, Spark schema/casts
и экспорт одного файла. Export возвращает пустой список result tables и обычный
Completed; табличного выхода у него нет. Отчёт о каталоге — результат команды,
не дополнительный порт export-ноды.

## §6. Ограничения и семантика

- None использует обычную identity клиента, Simple явно передаёт username.
  Simple не является защищённым password login. Kerberos/delegation tokens
  не поддержаны. SWebHDFS использует TLS Hadoop/JDK; отдельная TLS-приёмка NOT_RUN.
- Charset задаётся явно: UTF-8, windows-1251/1252, US-ASCII, UTF-16LE/BE.
  Некорректные/непредставимые символы дают ошибку, без замены символами `?`.
- Match all означает literal separator; выключенный — набор отдельных символов.
  Экспорт в этом случае пишет первый символ. Ignore consecutive схлопывает
  серии field separators, сохраняя один разделитель. Skip rows считается по
  неотброшенным blank records до header; max rows не включает header, 0 — без лимита.
- Column overrides используют исходные 1-based индексы. Типы: auto/string/long/
  double/boolean/date. Даты используют Java patterns, например `dd.MM.uuuu`.
  Поддержаны rename, omit, ограничение длины, очистка числовых полей и подстановка
  missing value. Сначала применяется missing replacement, затем преобразование.
- Null marker — текстовый контракт. Чтобы различать null и пустую строку,
  выберите непустой marker, который не встречается в обычных значениях.
- Нет Spreadsheet value labels, long names, row names и display formats;
  UI явно объясняет это, не создаёт неработающие переключатели.
- Пределы: 10000 колонок, 16 Mi UTF-16 units на record, max_field_chars до 16 Mi.
  Источник нормализует файл на локальный диск; нужны место и время O(размер файла).
  Кэшированные source spools хранятся до инвалидации/закрытия worker, обычные
  удаляются после запроса. Принудительное завершение JVM может оставить spool.
- Экспорт `toLocalIterator` расходует память вплоть до крупнейшей Spark partition;
  не заявляется универсальный bounded-RAM режим. Строковые/числовые, boolean,
  date/timestamp колонки поддержаны; массивы, maps, structs и ML-векторы для
  текстового экспорта отклоняются, их надо явно преобразовать/выбрать заранее.

## §7. Запуск и проверка

Готовый учебный стенд из `for agent hdfs` проверен после публикации DataNode
9864 и настройки hostname=localhost. Порт 9870 нужен NameNode, 9864 — redirects
чтения/записи. RPC 8020/9866 для этого WebHDFS-клиента публиковать не требуется.
Новый одноразовый стенд находится в `examples/hdfs/docker-compose.yml`; не
запускайте оба стенда одновременно на одинаковых портах. Новый стенд bind только
loopback, auth Simple; удаление его контейнеров удаляет данные.

```text
docker compose -p datacraft-hdfs -f examples/hdfs/docker-compose.yml up -d
```

В `workers/spark`: `sbt test writeClasspath`. Реальные network tests включаются
переменной `DATACRAFT_HDFS_TESTS=1`; без неё они явно пропускаются. Затем из корня:

```text
cargo run -p datacraft-cli -- run examples/pipelines/hdfs_export_text.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/hdfs_import_text.dcraft
cargo run -p datacraft-engine --example hdfs_lifecycle
cargo test -p datacraft-desktop --features hdfs-integration-tests hdfs_run_button -- --test-threads=1
```

Demo export создаёт `/datacraft/examples/sales.tsv`; повторный запуск требует
явного overwrite либо нового пути. Lifecycle/GUI acceptance создают свои UUID
каталоги `/datacraft-examples/` и `/datacraft-gui/`, намеренно оставляя demo файлы
для просмотра. Scala network tests используют отдельные UUID namespaces
`/datacraft-tests/` и удаляют только созданные ими данные.

## §8′. Устойчивость

Сетевые connect/read timeout задаются в inspector, общий job deadline/Cancel
остаётся в engine. Все операции выполняются вне UI thread. Неполная загрузка
не публикуется как готовый файл; обработанные ошибки удаляют только собственный
temporary path. Статические ошибки параметров выявляются до pipeline execution.
При недоступном DataNode требуется проверить адрес redirect, а не только
доступность NameNode Web UI. Фактические результаты: [VALIDATION.md](VALIDATION.md).
