# Проверки первой реализации

## Actions коммита 02826a8 и исправление проверки путей — 2026-11-12

[Run 43176628251](https://github.com/alexander1221/DataCraft/actions/runs/43176628251):
Spark job PASS целиком, включая sbt test, настоящий HDFS, fixtures, GUI tests
и lifecycle examples. Rust job FAIL в тесте
`parquet_export_save_as_preserves_destination_across_undo_and_redo`:
Linux сохраняет `new/../old` в пути отсутствующего файла; тест сравнивал строки.
Последующие Rust Clippy/architecture и native smoke в этом job не запускались.

Тест исправлен: проверяет абсолютность, имя назначения и canonicalize существующего
родительского каталога, не создавая output file. Production code не изменён.
Локально PASS: все 17 Session tests, fmt check и git diff check.
Удалённый Linux-прогон исправления NOT_RUN до нового commit/push.

## HDFS Text и локальный/HDFS Parquet — 2026-11-12

verified 2026-11-12 via Windows 11/MSVC, Rust 1.99.1, Temurin JDK 21.0.13,
Scala 2.13.16, Spark 4.0.1 и настоящий Hadoop 3.3.6 NameNode/DataNode.
Контракты: [HDFS_TEXT.md](HDFS_TEXT.md), [PARQUET_IO.md](PARQUET_IO.md).

| Проверка | Результат |
|---|---|
| `cargo xtask --offline ci` | PASS: fmt/check/test/Clippy, layers/commands/nodes; 131 Rust tests, один subprocess fixture ignored и отдельно вызывается тестом |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | PASS |
| Scala production/tests compilation | PASS: установленный Scala compiler, cached classpath |
| Scala/MUnit через JUnit, `DATACRAFT_HDFS_TESTS=1` | PASS: 60 tests, включая реальные WebHDFS OPEN/CREATE redirects |
| GUI `--features hdfs-integration-tests run_button -- --test-threads=1` | PASS: 9 сценариев, включая HDFS Text и оба Parquet round-trip |
| `cargo xtask --offline fixtures` | PASS: все 16 известных pipeline, схемы/значения совпали с независимыми эталонами |
| CLI `parquet_export`, `parquet_import`, `hdfs_export_parquet`, `hdfs_import_parquet` | PASS: стандартный worker classpath, успешные sinks без таблиц, оба import вернули ожидаемые 12 строк |
| `hdfs_lifecycle` example | PASS: export/import, 12 точных строк, browser/отмена не меняют результаты и persistent cache |
| Native desktop `hdfs_export_parquet.dcraft --smoke-frames 3` | PASS: 3 frames, project loaded |
| `validate_generated.py .` | PASS: 0 errors; это дополнительная проверка файлов, не замена компиляции |
| `sbt test writeClasspath` в sandbox | NOT_RUN: boot-cache lock недоступен; реальная compilation/MUnit выполнены напрямую теми же cached Scala/Spark dependencies |
| Ручной OS picker, визуальная приёмка всех новых настроек | NOT_RUN: GUI tests используют egui input и injected picker replies |
| Linux/macOS нового пакета, удалённые Actions, TLS/Kerberos | NOT_RUN |
| Независимое ревью другим человеком/агентом | NOT_RUN |
| `git diff --check` | PASS |
| Staging/commit | BLOCKED: `.git/index.lock: Permission denied`; sandbox предоставляет `.git` только для чтения. Изменения остаются в рабочей папке |

Независимые ожидания: Text — exact UTF-8 bytes, Unicode/quotes/nulls, ANSI/decimal/
date/skip/limit; ошибки прав/сети/формата, запрет overwrite и cleanup своих tmp.
Parquet — magic PAR1, footer row count/codec, Decimal(38,10), int64 > 2^53,
dates/timestamp, nulls, arrays/maps, ML VectorUDT и metadata. Проверены четыре
заявленных кодека, пустые таблицы и сохранность назначения при ошибке.
Worker wire и Rust ResultTable сохраняют точные numeric JSON tokens.

Найденные при разработке ошибки компиляции тестов, float-deserialization с
lossless JSON и wrapped Hadoop diagnostics исправлены; перечисленные финальные
прогоны прошли. HDFS stand запущен пользователем; Docker API агенту недоступен.
Новый Compose прошёл `config --quiet`, запуск именно этого Compose локально
NOT_RUN. Тесты использовали пользовательский stand с опубликованным DataNode.
Workflow включает собственный disposable stand; удалённый успех не заявляется.

## Контуры статусов без цветной заливки — 2026-11-09

PASS: 30 desktop unit tests, fmt check, Clippy desktop all-targets/all-features
с `-D warnings`. Проверка canvas подтверждает обычную тёмную заливку,
зелёный/красный контур и сохранение spinner. Spark-интеграция для этой правки
оформления NOT_RUN; её assertion обновлён на контур и скомпилирован.

## Поузловое выполнение и оформление canvas — 2026-11-09

verified 2026-11-09 via Windows/MSVC, Rust 1.99.1, Java 21.0.13,
Scala 2.13.16, Spark 4.0.1. Контракт: [NODE_EXECUTION.md](NODE_EXECUTION.md).

| Проверка | Фактический результат |
|---|---|
| cargo xtask --offline ci | PASS: fmt/check/test/Clippy, layers, commands, nodes |
| cargo test --workspace --locked --offline | PASS: 120 тестов; один ignored subprocess fixture вызывается родительским тестом |
| Clippy all-targets/all-features, -D warnings | PASS |
| Реестры | PASS: 20 команд, 27 типов нод; новые execution.node.run / execution.downstream.run |
| Scala compiler + JUnitCore | PASS: 44 настоящих Spark/MUnit-теста, включая четыре NodeExecutionSuite |
| cargo xtask fixtures | PASS: 16 эталонных pipeline, схемы и значения совпадают с независимыми oracles |
| Opt-in GUI run_button tests | PASS: все 6 сценариев с реальным Spark; новый тест нажимает ▶ / ⏩ через egui input, проверяет строки и красную подсветку |
| node_lifecycle example | PASS: строгие входы, сохранённый worker, quantity 4/5/6, инвалидация, ошибка, undo, cancel и восстановление |
| job_lifecycle example | PASS: прежний режим, отмена JVM и обнаружение устаревшего результата |
| Native sample --smoke-frames 4 | PASS: проект загружен, четыре кадра, exit 0; не заменяет ручную визуальную приёмку |
| validate_generated.py . | PASS: errors=0 |
| sbt test writeClasspath | FAIL до компиляции: sandbox не разрешает sbt.boot.lock; проверка compiler/JUnit выполнена отдельно |

Проверены настоящие materialized таблицы: временный CSV удалён после Source,
следующий Filter успешно вернул 4/5/6 из кэша. Проверены повторное использование
fitted Model, ветвящийся preprocessing/classification DAG и оба выхода Split.
Rust state-machine tests отдельно проверяют независимую ветку, invalidation
по config/рёбрам/родителям, игнорирование поздних событий старого snapshot,
различие lazy success и cached inputs, порядок/идентичность protocol frames.
Canvas tests проверяют footer hit targets без Move/Select, disabled controls,
цветовые состояния и геометрию spinner.

NOT_RUN для этого пакета: ручная визуальная приёмка нового оформления,
новый удалённый GitHub Actions run, Linux/macOS и независимый reviewer.
Новых зависимостей нет. Сохранение кэша между открытиями и надёжный checkpoint
не реализованы; MEMORY_AND_DISK допускает внутренний Spark recomputation.
ML tolerance 1e-6 остаётся provisional acceptance, не оценкой ошибки solver.

Ниже сохранена история предыдущих проверок; старые NOT_RUN не отменяют
результаты актуального пакета выше.

Дата: 2026-11-08. Среда: Windows, PowerShell, Python 3.14.3.

## Отбор признаков: 27 типов нод

verified 2026-11-08 via Windows/MSVC, Rust 1.99.1, JDK 21.0.13,
Scala 2.13.16, Spark 4.0.1, MUnit 1.0.4/JUnit 4.13.2.
Новые kind: feature_selection, feature_selection_pipeline, random_forest_pipeline.
Контракт: [FEATURE_SELECTION.md](FEATURE_SELECTION.md).

| Проверка | Фактический результат |
|---|---|
| cargo xtask --offline ci | PASS: fmt/check/tests/Clippy, layers, commands, nodes |
| Workspace tests | PASS: 111 тестов, один subprocess fixture ignored, вызывается родительским тестом |
| Clippy --workspace --all-targets --all-features --locked --offline -- -D warnings | PASS |
| Scala compiler + JUnitCore | PASS: 40 реальных Spark/MUnit-тестов, включая 32 предыдущих |
| cargo xtask fixtures | PASS: 16 проектов, схемы/значения совпадают с независимыми oracles |
| CLI feature_selection.dcraft | PASS: signal χ²≈12, df=4, p≈0.017351265236664526; noise χ²=0, df=2, p=1 |
| CLI feature_selection_pipeline.dcraft --output ...000006 | PASS: train-only indices 0/1/3 для test a/b/unseen d |
| CLI random_forest_pipeline.dcraft --output ...000011 | PASS: downstream classifier accuracy и weighted метрики = 1 |
| Opt-in selection_run_button | PASS: два JVM jobs через GUI adapter, importance 0/1, test selected vectors 1/7, metadata dimension=1 |
| Native random_forest_pipeline.dcraft --smoke-frames 4 | PASS: проект загружен, четыре кадра, exit 0 |
| sbt test writeClasspath | FAIL до компиляции: sandbox запрещает доступ к sbt.boot.lock |

Проверены χ² по аналитической contingency table, unlabelled Transform, unseen
category без переобучения, сохранение исходных continuous values вместо bins,
имена/порядок выбранных features, copy модели, constant df=0 с null p-value,
детерминированные ties. RF classification/regression importance проверена на
signal/constant noise; sparse vectors, неправильная test dimension и запрет
Predict для selection Model проверены отдельно. Полный RF selection→classifier
исполнен реально. Изменение только test target сохраняет selection и превращает
perfect classification в четыре нулевые метрики — отдельный leakage-тест.

Невалидные lists/subsets/лимиты/NaN defaults отклоняются в Rust без JVM.
Backend проверяет nulls, incompatible types, неизвестные категории в strict mode,
пустой train, высокую cardinality, output collisions и недостаточное количество
неconstant features. Временные имена резервируются с учётом пользовательского
output name; отдельный тест с __datacraft_selection_3 подтверждает отсутствие
коллизии и потери output. Новые проекты включены в round-trip/path и inspector tests.

Первый Scala прогон — FAIL: Spark degreesOfFreedom вернул mutable ArraySeq,
а код ожидал immutable Seq; VectorSizeHint требовал literal name без backticks.
Оба API-различия исправлены. Следующий прогон выявил потерю feature names:
VectorAssembler.transformSchema не заполняет attrs. Metadata теперь восстанавливается
из исходных имён и fitted vocabulary с сохранением Nominal/Numeric типов.
После исправлений и защиты temporary names повторён полный suite: 40 PASS.

Численный отчёт для двух categorical predictors: L∞ absolute error statistic
=1.7763568394002505e-15 относительно [12,0], p-values =1.734723475976807e-17
относительно [7*exp(-6),1]. Отношения к provisional acceptance 1e-12:
0.0017763568394002505 и 0.00001734723475976807. Это наблюдения на tiny oracle,
не доказательство общего error bound или пригодности asymptotic p-values для
малых expected counts. Importance 0/1, selected flags, indices и выбранные
test values сравниваются точно. Старый solver threshold 1e-6 не менялся.

Fallback использует установленные Scala compiler/MUnit jars и JVM flags из
build.sbt; 13 проверенных production .class скопированы только в workspace target.
Отдельные CLI-прогоны используют обычный target/classpath.txt. Это не успешный
sbt run. Новых зависимостей нет; на свежем checkout используйте sbt test writeClasspath.

Логи (ignored): `.tools/selection-{ci,scala,fixtures,cli-report,cli-transform,
cli-metrics,ui-spark,clippy-features,native,sbt,arrange}.log`. После уточнения
temporary-name reservation повторены Scala suite и затронутый CLI Transform.
NOT_RUN: новый Actions run, новые ноды на Linux/macOS, независимое review,
ручная приёмка новых нод в настоящем окне. Native startup и GUI adapter проверены
отдельно. Job lifecycle в этом пакете отдельно не повторялся.
Ограничения: categorical target для χ², unadjusted/asymptotic p-values,
provisional 1e-12, bounded metadata вместо общей RAM-гарантии, runtime-only models.

## Читаемость графа и актуальность результатов

verified 2026-11-08 via Windows/MSVC и локальные команды.
Контракт: [EDITOR_READABILITY.md](EDITOR_READABILITY.md).

| Проверка | Фактический результат |
|---|---|
| cargo xtask --offline ci | PASS: fmt/check/tests/Clippy, layers, commands, nodes |
| Workspace tests | PASS: 108 тестов, один subprocess fixture ignored (вызывается родительским) |
| Clippy --workspace --all-targets --all-features --locked --offline -- -D warnings | PASS |
| Scala compiler + JUnitCore | PASS: 32 реальных Spark/MUnit-теста после пересохранения раскладки |
| Opt-in readability_run_button | PASS: два JVM jobs, Move/Rename во время Run, последующие Move/Undo, Configure invalidation, повторный Run |
| job_lifecycle | PASS: отмена с cleanup и recovery; stale проверяется изменением Filter вместо Rename |
| Native logistic_regression.dcraft --smoke-frames 4 | PASS: project loaded, четыре кадра, exit 0 |

Arrange протестирован через настоящий egui button: все moves — один Undo,
параметры/связи не меняются. Pure layout tests проверяют детерминированность
при перестановке nodes/edges, отсутствие пересечения node rectangles,
возрастание X по рёбрам и разнесение train/test. Инструмент arrange_projects
использует только Session Open/Batch/Save; прямой зависимости UI→project нет.
Изменены координаты 11 поставляемых fit/transform-примеров. sample.dcraft
пользователя и AGENTS.md не менялись. Open не переставляет пользовательские ноды.

В real Spark UI test измерены координаты ячеек первой test row: между правым
краем id=9 и левым краем x=-2.5 минимум 20 logical pixels. Move/Rename во время
job сохраняют current result несмотря на различие project revision. После
Configure результат stale; computational Undo остаётся консервативно stale,
новый Run снова даёт current. Unit state fixtures отдельно проверяют Save as
rebasing и atomic failures; они не выдаются за Spark-исполнение.

Первоначальные FAIL исправлены: Clippy потребовал вынести state tests в отдельный
модуль; architecture checker отклонил временную dev UI→project зависимость —
utility/tests переписаны через Session. UI test сначала проверял last_execution
до финального polling frame: terminal status мог появиться между кадрами;
добавлен финальный frame, затем проверен результат. Все указанные gates повторно PASS.

Попытка PrintWindow capture — NOT_USABLE: сначала MainWindowHandle указывал на
маленькое служебное окно, затем правильное окно Glow дало белый client area.
Это не визуальная приёмка исправленной раскладки. Тестовый процесс закрыт через
WM_CLOSE по окну DataCraft, принадлежащему только его PID. Отдельный native
smoke успешно завершился с exit 0; геометрия/содержимое проверены egui tests.

Логи (ignored): `.tools/readability-{ci,ui-spark,scala,lifecycle,clippy-features,
native,build,arrange}.log`. Новый удалённый Actions run, Linux/macOS,
независимое review и ручная приёмка новой геометрии — NOT_RUN. Полный fixtures
прогон в этой итерации отдельно не повторялся; Spark suite и preview/recovery
исполнялись реально. Worker production-код и dependencies не менялись.
Раскладка не гарантирует ноль пересечений; external data changes не отслеживаются.

## Пакет классификации: 24 типа нод

verified 2026-11-08 via Windows/MSVC, Rust 1.99.1, JDK 21.0.13,
Scala 2.13.16, Spark 4.0.1, MUnit 1.0.4/JUnit 4.13.2.
Новые kind: logistic_regression, linear_svc, decision_tree_classifier,
random_forest_classifier, classification_metrics. Контракт и независимые
ожидания: [CLASSIFICATION.md](CLASSIFICATION.md).

| Проверка | Фактический результат |
|---|---|
| cargo xtask --offline ci | PASS: fmt, check, tests, Clippy, layers, commands, nodes |
| Workspace tests | PASS: 103 теста; один subprocess fixture ignored, запускается родительским тестом |
| Clippy --workspace --all-targets --all-features --locked --offline -- -D warnings | PASS, включая новый opt-in GUI test |
| Scala compiler + JUnitCore | PASS: 32 настоящих MUnit-теста, включая 24 предыдущих |
| cargo xtask fixtures | PASS: 13 проектов, схемы и scalar values совпадают с независимыми oracles |
| CLI Logistic Regression, обычный target/classpath.txt | PASS: accuracy и три weighted метрики = 1 |
| CLI Linear SVM --output ...000009 | PASS: четыре test predictions 0/0/1/1 |
| Opt-in classification_metrics_run_button | PASS: mouse click запускает настоящий JVM, четыре метрики отрисованы |
| Native logistic_regression.dcraft --smoke-frames 4 | PASS: проект загружен, четыре кадра, exit 0 |
| sbt test writeClasspath | FAIL до компиляции: отказ доступа к sbt.boot.lock в sandbox |

Scala-проверки: все четыре binary pipeline исполняют train→preprocessing→Predict
на test; Tree/Forest также проверены на трёх классах и entropy. Logistic threshold
0/1 меняет решения; probability имеет две конечные компоненты в [0,1]. LinearSVC
возвращает rawPrediction без выдуманной probability. Bootstrap forest повторяет
предсказания с одинаковым seed в данном Spark runtime.

Для сбалансированного confusion oracle все четыре метрики точно 0.5; perfect
multiclass и perfect 1000-class оценка дают ровно 1. Неверная классификация
единственного target-класса даёт 0 без деления на ноль. После изменения только
test labels предсказания 0/0/1/1 сохранены, а все метрики стали 0. Это отдельный
leakage-тест, не snapshot собственного вывода.

Проверены null/NaN/Infinity, отрицательные/дробные/out-of-int32 IDs, пустые
данные, неправильные схемы, reserved columns, разреженная нумерация train
labels, multiclass для binary estimator, invalid impurity и >1000 labels/cells.
Эти случаи возвращают recoverable errors; после ошибки проверен успешный запрос.
Сборка warning-free; старые regression numerical assumptions не менялись.

Metrics суммирует weighted числители до деления на n, чтобы простой perfect
набор с большим числом классов не накапливал сумму округлённых долей >1.
Weighted recall выводится из тождества с accuracy для single-label задачи.
После этого уточнения повторён полный Scala suite и проверки затронутых примеров.
Это не гарантия точности Double-агрегаций на произвольных данных.

Новые четыре проекта включены в round-trip/path tests и egui inspector rendering;
там каждый узел проверен через ParameterEditor без Java. Отдельный opt-in тест
использует настоящий worker; ручной Run в настоящем окне — NOT_RUN, native
startup проверен отдельно. CI Run-button filter автоматически включает этот тест.

Fallback Scala build использует установленные compiler/MUnit jars, те же JVM
add-opens/Xmx2g, что в build.sbt. Восемь проверенных production .class скопированы
только в локальный workspace target; новые Maven/Cargo dependencies не добавлены.
Это не успешный sbt run; на свежем checkout используйте `sbt test writeClasspath`.

Логи (ignored): `.tools/classification-{ci,scala,fixtures,cli-metrics,cli-preview,
ui-spark,clippy-features,native,sbt}.log`; compiler/JUnit args в .tools/preprocessing.
Повторные CLI-прогоны после уточнения weighted суммирования:
`.tools/classification-final-{logistic_regression,linear_svc,decision_tree_classifier,random_forest_classifier}.log`.
NOT_RUN: новый удалённый Actions run, новые ноды на Linux/macOS, независимое
review, полный `ci --with-spark` через sbt; Job lifecycle отдельно не повторялся.
Ограничения: binary Logistic/SVC, L2 SVM вместо legacy L1, numeric contiguous
train labels, до 1000 занятых confusion cells, без ROC/AUC и сохранения моделей.

## Пакет регрессии: 19 типов нод

verified 2026-11-08 via Windows/MSVC, Rust 1.99.1, JDK 21.0.13,
Scala 2.13.16, Spark 4.0.1, MUnit 1.0.4/JUnit 4.13.2.
Новые executable kind: generalized_linear_regression, decision_tree_regressor,
random_forest_regressor, regression_metrics. Контракт: [REGRESSION.md](REGRESSION.md).

| Проверка | Фактический результат |
|---|---|
| cargo xtask --offline ci | PASS: fmt, workspace check/tests/Clippy, layers, commands, nodes |
| Обычные workspace tests | PASS: 101 тест, один subprocess fixture ignored (запускается родительским тестом) |
| cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings | PASS, включая новый opt-in GUI test |
| Scala main/tests compiler и JUnitCore | PASS: 24 MUnit-теста с настоящим Spark; deprecation warnings устранены |
| cargo xtask fixtures | PASS: девять проектов, схемы и значения соответствуют независимым oracles |
| CLI random_forest.dcraft, обычный target/classpath.txt | PASS: RMSE/MSE/MAE=0, R²=1 |
| CLI random_forest.dcraft --output ...000009 | PASS: две test rows, prediction=2/10 при x=1/7 |
| Opt-in regression_metrics_run_button test | PASS: два настоящих JVM jobs через GUI adapter, метрики и null R² отрисованы |
| Native random_forest.dcraft --smoke-frames 4 | PASS: окно загружает проект, четыре кадра, exit 0 |
| sbt test writeClasspath | FAIL до компиляции: sandbox запрещает доступ к sbt.boot.lock |

Новые Scala-тесты проверяют четыре GLM family на известных постоянных response,
GLM gaussian на y=2x+3, Tree/RF на аналитически заданных листьях, реальный
fit/predict/metrics protocol, repeated bootstrap forest с seed. Для labels=[1,3],
predictions=[2,2] точно проверены RMSE=MSE=MAE=1, R²=0; constant/single target
возвращает R²=null. Ошибки пустого набора, null/NaN/Infinity, overflow, типов
схемы и GLM domain не заменяются исключением строк или нулевыми метриками.

Отдельный leakage-тест меняет только test labels на 5: predictions остаются
2/10, MSE становится точно 17, MAE=4, R²=null. Это проверяет подключение test
ветки, а не только способность estimator вернуть модель.
Model output новых estimator-нод читается общим Predict; прямой отчёт по
коэффициентам/feature importance для них пока не реализован.

Opt-in команда: `cargo test --offline --locked -p datacraft-desktop --features
spark-integration-tests regression_metrics_run_button -- --test-threads=1`.
Это egui adapter, принимающий mouse events и запускающий настоящий worker;
ручной Run в настоящем окне для новых estimator-нод отдельно — NOT_RUN.
Native startup проверен отдельно. CI дополнен обоими Run-button сценариями;
новый удалённый Actions run не запускался.

Fallback Scala build использует уже установленные compiler/runtime/MUnit jars
и аргументы JVM из build.sbt, как в предыдущем разделе. Собраны шесть production
.class, они скопированы только в локальный workspace target; обычный classpath
подтверждён CLI. Это не успешный sbt run. Новых dependencies не добавлено;
на свежем checkout используйте штатный `sbt test writeClasspath`.

Первоначальные FAIL исправлены: неоднозначный Scala .toMap.apply в тесте,
Clippy match-like-matches-macro и сравнение JSON integer/float токенов в UI-тесте.
Для результата явно задаётся value: Double schema и проверяется численное
значение независимо от лексического представления JSON (0 либо 0.0).
Preview восстанавливает пропущенные Spark JSON null cells по top-level schema;
это проверено на неопределённом R². После исправлений suite прошли повторно.

Логи (ignored): `.tools/regression-{ci,scala,fixtures,cli-metrics,cli-preview,
ui-spark,clippy-features,native,sbt}.log`. Compiler/JUnit argument files находятся
в `.tools/preprocessing/`. Границы: новые ноды на Linux/macOS, независимое
review и полный `ci --with-spark` через sbt — NOT_RUN; прежний Job lifecycle
в этой итерации отдельно не повторялся.
ASSUMPTION: threshold 1e-6 для GLM и Linear Regression остаётся provisional,
не solver error bound. Для Tree/RF fixtures и точного residual oracle допуск 0.
Ни seed, ни эти тесты не доказывают общую численную эквивалентность старым нодам.

## Подготовка данных и признаков: 15 типов нод

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1,
JDK 21.0.13, Scala 2.13.16, Spark 4.0.1, MUnit 1.0.4/JUnit 4.13.2.
Добавлены JSON Source (NDJSON), Train/Test Split, Make Design Matrix,
Index Categoricals, Reference Code Categoricals, Transform. Подробный контракт
и происхождение ожидаемых значений: [PREPROCESSING.md](PREPROCESSING.md).

| Проверка | Фактический результат |
|---|---|
| cargo xtask --offline ci | PASS: fmt/check/tests/Clippy, layers, commands, nodes |
| cargo test --workspace --locked (offline, внутри ci) | PASS: 98 тестов; один subprocess fixture ignored, вызывается родительским тестом |
| cargo xtask fixtures | PASS: шесть проектов, схемы и scalar values соответствуют независимому oracle |
| Scala main + tests, прямой компилятор и JUnitCore | PASS: 17 настоящих MUnit-тестов, включая прежние шесть |
| CLI run reference_code.dcraft | PASS: known a→[1,0], unseen c→[0,0], числовой x объединён с вектором |
| CLI run train_test.dcraft | PASS: test preview, в данном запуске 32 строки из 128 |
| CLI inspect features.jsonl --rows 2 | PASS: две строки, JSON schema с long/string |
| CLI run index_categoricals.dcraft, обычный target/classpath.txt | PASS: id=5/a/0, id=6/c/2 |
| Desktop train_test.dcraft --smoke-frames 4 | PASS: настоящий native startup, project loaded, exit 0 |
| Job lifecycle | PASS: cancel после JVM cleanup, recovery и обнаружение stale result |
| validate_generated.py и git diff --check | PASS: ошибок артефактов/whitespace нет |

Архитектурные проверки нашли 18 command IDs и 15 node definitions. Desktop-тест
открывает все четыре новых проекта, создаёт инспекторы каждого узла и проверяет
видимые train/test порты без запуска Java; JSON Browse включён в adapter-тест.
Ручное выполнение новых графов кнопками в настоящем окне — NOT_RUN;
native startup и headless UI adapter проверены отдельно.

Scala-проверки включают disjoint/exhaustive/grouped split, null-key grouping,
инвариантность к repartition/order, изменение seed; train-only vocabulary,
unknown/null bucket, точные sparse/dense 0/1 vectors, RFormula order/metadata,
unlabeled Transform, numeric+vector assembly. Ошибки неизвестной категории,
пустого fit, malformed formula/NDJSON, collisions и неправильного типа модели
возвращаются recoverably; после ошибки проверен следующий успешный запрос.
Spark ERROR в логах намеренно невалидных данных не означает падение тестового suite.

`sbt test writeClasspath` — FAIL до компиляции: sandbox не даёт создать/открыть
`AppData/Local/sbt/boot/sbt.boot.lock`. Права и глобальные настройки не менялись.
Для фактической проверки использован установленный Scala compiler 2.13.16
(`scala.tools.nsc.Main`) с Maven jars из уже заполненного Coursier cache;
MUnit запущен через `org.junit.runner.JUnitCore` с теми же JVM add-opens/Xmx2g,
что в build.sbt. Main/test classes собраны в `.tools/preprocessing/`;
проверенные четыре production .class скопированы в локальный workspace target.
Первый fixtures-прогон использовал DATACRAFT_WORKER_CLASSPATH на этот build;
обычный classpath затем проверен отдельным CLI-запуском. Это не успешный sbt run.
На свежем checkout остаётся штатная команда `sbt test writeClasspath`.

Первоначальные FAIL исправлены: шесть строк не гарантируют непустой test,
поэтому split-пример использует 128 строк; round-trip тест учитывает каноническую
сортировку рёбер; preflight-тест явно конфигурирует worker; UI assertion использует
постоянные port IDs train/test. После исправлений соответствующие suite прошли.

Логи (ignored): `.tools/preprocessing-{ci,scala,fixtures,reference_code,train_test,
inspect,default-classpath,native,lifecycle,sbt}.log`; аргументы compiler/JUnit
сохранены в `.tools/preprocessing/*.args`. Новых Maven/Cargo зависимостей для
preprocessing не добавлено. Rfd относится к предыдущему пакету диалогов ниже.

NOT_RUN: новый удалённый Actions run, новые ноды на Linux/macOS, независимое
review, полный запуск `ci --with-spark` через sbt в этой sandbox.
ASSUMPTION: старый ML threshold 1e-6 остаётся provisional acceptance contract,
не solver error bound. Для новых индексов/векторов и scalar design fixtures
допуск нулевой. Split не обещает точную долю или неизменный hash между версиями
Spark; fit не имеет общей гарантии bounded RAM. Модели пока не сохраняются.

## Системные диалоги и Rename

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1,
eframe/egui 0.33.3, rfd 0.18.0, JDK 21.0.13:

- Добавлен rfd 0.18.0 (MIT, MSRV 1.92). API/default features проверены по
  исходникам опубликованного tag; настоящий Cargo обновил Cargo.lock.
  Первый fetch в агентской сессии — FAIL из-за недоступного index.crates.io;
  пользователь выполнил fetch, дальнейшие сборки — --locked --offline.
- `cargo xtask ci --offline` — PASS: fmt/check/test/Clippy, layers, 18 commands,
  9 nodes. Workspace: 93 теста; одна ignored-функция по-прежнему запускается
  как subprocess fixture тестом отмены процесса.
- Desktop: 24 обычных теста, включая pending/ready/cancel native future model,
  запрет второго picker, Open с защитой неприменённого буфера, отмену всех видов
  picker без изменения проекта/буфера/истории, stale revision/UUID, Browse
  CSV/Parquet только в буфер, Save as с Unicode именем и rebasing, защиту
  future-version файла, Save and close нового проекта с отменой, Rename,
  Undo/Redo, пустое имя, no-op, запрет закрытия родителя при открытом picker.
  Ответы selector в этих adapter-тестах подаются явно, ОС-диалоги они не открывают.
- Дополнительный opt-in Spark UI-тест — PASS, два настоящих JVM jobs:
  исходные (4,11)/(5,13)/(6,15), затем ответ selector с абсолютным Unicode-путём
  к реальной копии CSV, Apply, Filter quantity>=5 и повторный Run:
  (5,13)/(6,15)/(7,17). Unicode + пробелы прошли через Rust/JSON/JVM/Spark.
  Этот тест не входит в обычные 93. Сам selector здесь также инъецирован.
- Clippy desktop с feature spark-integration-tests — PASS.
- Нативное окно с новым toolbar отрисовано; screenshot Glow backend просмотрен:
  `.tools/file-dialog-window.png` (ignored).
- Настоящие Windows Open и Save as — PASS на открытие/отмену: inline PowerShell
  с Win32-сообщениями только HWND запущенного тестового PID нашёл окна
  Open DataCraft project / Save DataCraft project as, закрыл каждый через Cancel,
  затем DataCraft завершился с exit 0. Никакие файлы в этом native smoke не
  выбирались и не сохранялись. Попытка запуска сохранённого .ps1 helper сначала
  дала FAIL до исполнения из-за ExecutionPolicy; настройки не менялись, проверка
  выполнена обычными inline-командами. Вывод сохранён в истории вызова инструмента.
- Scala worker не менялся; sbt tests, четыре fixtures и job_lifecycle отдельно
  повторно — NOT_RUN. Свежая проверка реального backend в этой итерации — UI-тест.
- Ранее проверенный [Actions 7a7d60f](https://github.com/alexander1221/DataCraft/actions/runs/42485500996)
  прошёл целиком, включая Linux native window под Xvfb. Новые rfd picker на
  Linux/macOS, новый удалённый workflow и независимое review этой итерации — NOT_RUN.

Логи: `.tools/file-dialog-ci.log`, `file-dialog-tests.log`, `file-dialog-spark.log`,
`file-dialog-build.log`, `file-dialog-native.log`, `file-dialog-clippy-spark.log`.
Нативный выбор конкретного файла и весь ручной сценарий Save as/Browse/close
ещё требуют приёмки; открытие и отмена Windows popup проверены отдельно выше.
Переименование не меняет имя файла. Native Source выбирает абсолютный путь;
для переносимого проекта пользователь может ввести относительный путь вручную.
Roadmap переставлен: следующий пакет — подготовка данных/признаков Spark,
дальнейшая полировка GUI не является предварительным условием.
ASSUMPTION: прежний ML threshold 1e-6 не изменён, solver error bound не обоснован.

## GitHub Actions для 65bf9b0: Linux GUI prerequisite

verified 2026-11-08 via GitHub Actions API, jobs/steps и лог rust job:
[checks #4](https://github.com/alexander1221/DataCraft/actions/runs/42483156203).

- Общий workflow — FAIL.
- cargo xtask ci в rust job — PASS: fmt/check/test/Clippy, 81 Rust-тест,
  layers, 18 commands, 9 nodes. Два следующих CLI discovery steps — SKIPPED
  после отказа native smoke, не PASS.
- spark job — PASS целиком: Scala tests, CSV/draft validation, четыре fixtures,
  UI input → настоящий Spark (включая изменение Filter), inspect, job_lifecycle.
- Linux native smoke под Xvfb — FAIL: xkbcommon-dl-0.4.2/src/x11.rs,
  `Library libxkbcommon-x11.so could not be loaded`, exit 101.
  Локальный исходник loader подтверждает попытку загрузить
  libxkbcommon-x11.so.0 / libxkbcommon-x11.so. Это отдельная библиотека X11,
  существующего libxkbcommon-dev в runner оказалось недостаточно.
- В rust job добавлен пакет libxkbcommon-x11-dev. Xvfb/native smoke сохранён;
  отказ не маскируется continue-on-error или пропуском проверки.
  Повторный Linux запуск с исправленным workflow — NOT_RUN до нового push.

## Редактор нод, параметров и связей

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1,
eframe/egui 0.33.3, JDK 21.0.13:

- `cargo xtask ci --offline` — PASS: fmt/check/test/Clippy, слои,
  18 command contracts и 9 NodeSpec. 81 Rust-тест, одна subprocess fixture ignored
  (выполняется отдельным процессом из теста отмены).
- Новые проверки desktop — PASS: typed round-trip всех нод; ошибки u32/f64,
  NaN/Inf, Boolean, строки/списки; input events инспектора с Apply/Revert и Undo;
  сохранение буфера при смене selection/New/Open; отказ Save при ошибке числа
  без создания файла; палитра, Delete/Disconnect и Undo с incident edges;
  save/load неполного графа; output→input через pointer events, Escape;
  отказ при цикле/несовместимом типе без изменения графа/истории.
- Новые проверки engine — PASS: project.new защищает dirty/active job,
  проверяет имя, сбрасывает path/history/checkpoint, сохраняет пустой граф.
  New/Open после завершения job не возвращают старые результаты при polling.
  Для проверки lifecycle используется явно synthetic scheduler, не fake Spark.
- Отдельный opt-in `run_button_executes_real_spark_and_preview_is_rendered`
  с feature spark-integration-tests — PASS: Run через egui input и настоящий
  Spark вернул (4,11)/(5,13)/(6,15). Затем UI изменил Filter на quantity>=5,
  Apply и повторный Run вернули точно (5,13)/(6,15)/(7,17). New очистил результат.
  Это один дополнительный тест вне обычных 81, с двумя настоящими JVM jobs.
- Clippy desktop с feature spark-integration-tests — PASS.
- `cargo xtask fixtures --offline --java <JDK21/bin/java.exe>` — PASS:
  sample/draft/regression/prediction совпали с JSON oracle по схемам и значениям.
- Native startup sample с --smoke-frames 3 — PASS, exit 0, project loaded=true.
  Просмотрен новый снимок настоящего Glow window: палитра, New project, граф
  с пользовательской раскладкой. `.tools/graph-editor-window.png` ignored.
- Повторный `job_lifecycle` на настоящем JVM — PASS: cancellation confirmed
  after cleanup, следующий запуск успешен, stale revision 1/current 2 обнаружен.
- Scala worker не менялся, sbt tests повторно — NOT_RUN; их прежний PASS
  остаётся историческим. GitHub CI новой итерации, Linux/macOS GUI и независимое
  review новых UI/lifecycle изменений — NOT_RUN.

Сохранённый пользователем `examples/pipelines/sample.dcraft` содержит изменённые
координаты нод; файл не откатывался. Два старых Session-теста сначала упали из-за
предположения об исходной позиции (0,0). Их helper теперь задаёт собственную
детерминированную входную раскладку, независимо от редактируемого примера.
Тестовые assertions атомарности и Undo сохранены. Тест модального перехода
учитывает начальный sizing pass egui Area перед проверкой/нажатием кнопок.

Пользователь до этой итерации подтвердил native preview, движение нод и отсутствие
наблюдаемых зависаний; это приёмка предыдущего viewer, не всех новых жестов.
Ручная приёмка полного редактора/Save as/dirty-close/Cancel ещё нужна.
Логи: `.tools/editor-ci.log`, `editor-tests.log`, `editor-spark.log`,
`editor-fixtures.log`, `editor-native-final.log`, `editor-clippy-spark.log`,
`editor-lifecycle.log`. Дешёвый validator и git diff --check — PASS.
ASSUMPTION: ML threshold 1e-6 не изменён, его solver error bound не обоснован.

## Первый desktop-клиент

verified 2026-11-08 via локальные команды агента, Windows/MSVC, Rust 1.99.1,
eframe/egui 0.33.3, JDK 21.0.13:

- Первоначальный fetch в ограниченной сессии — FAIL: нет доступа к index.crates.io.
  Пользователь затем выполнил fetch в PowerShell: 264 crates загружены.
  Последующие Cargo checks выполнялись по реальному Cargo.lock из кэша.
- `cargo xtask ci --offline` — PASS: fmt/check/test/Clippy, layers, commands,
  nodes. 70 Rust-тестов (66 прежних + 4 desktop); subprocess fixture по-прежнему
  единственная ignored-функция. Desktop в layer=ui; headless crates без egui.
- Desktop tests — PASS: неподвижность Project при render/selection,
  сохранение проекта/selection/view при ошибке Open, сохранение точки при zoom,
  реальные pointer events drag с ровно одной Move action после release.
  Проверяется также, что строка Project path рисуется внутри toolbar clip rect.
- `cargo test --locked --offline -p datacraft-desktop --features
  spark-integration-tests run_button_executes_real_spark_and_preview_is_rendered`
  с DATACRAFT_JAVA=JDK21/bin/java.exe — PASS: отдельный opt-in тест нажимает
  Run через egui input, дожидается настоящего Spark и проверяет точные строки
  (4,11)/(5,13)/(6,15), одну таблицу и отрисованный preview с числом строк.
  Его нет в обычных 70 тестах, он запускался отдельно, без fake worker response.
- Дополнительный Clippy desktop с feature spark-integration-tests — PASS.
- Native startup `cargo run --locked --offline -p datacraft-desktop --
  examples/pipelines/sample.dcraft --smoke-frames 3` — PASS, exit 0:
  отрисованы нативные frames, project loaded=true, приложение завершилось.
- Снимок настоящего Glow window через диагностический feature eframe/__screenshot
  — PASS: видны пять нод, связи, путь проекта, кнопки, Inspector и Results.
  После просмотра исправлены тема и обрезание второй строки toolbar; добавлен
  regression assertion. Снимок: `.tools/desktop-window.png` (локальный, ignored).
- Scala-код не менялся; sbt tests и четыре fixture pipeline повторно не
  запускались в этой итерации. Свежий полный PASS baseline — предыдущий блок.
  Новый Spark UI-тест, в отличие от этих исторических проверок, исполнен сейчас.
- GitHub workflow дополнен Linux native smoke через Xvfb, системными GUI
  prerequisites и отдельным Spark UI-тестом. Этот обновлённый workflow — NOT_RUN
  удалённо; ранее прошедший Linux headless CI относится к ffb6288.

Логи: `.tools/desktop-ci-final.log`, `desktop-tests.log`, `desktop-spark-test.log`,
`desktop-native.log`, `desktop-screenshot.log`, `desktop-clippy-spark.log`.
Ручные native сценарии Save as/dirty-close/Cancel/Undo и независимое review
нового UI — NOT_RUN. Headless UI input test не заменяет эту ручную приёмку.
Создание/удаление нод, редактирование параметров/соединений, native file picker
не реализованы; параметры только для чтения. ASSUMPTION: прежний ML threshold
1e-6 не изменён и по-прежнему не является доказанной границей ошибки solver.

## Полный прогон xtask в пользовательском PowerShell

verified 2026-11-08 via предоставленный пользователем полный вывод
`cargo fetch --locked` и `cargo xtask ci --with-spark`, Windows/JDK 21.0.13,
завершение 14:29 по местному времени. Это отдельный пользовательский запуск,
не повторное выполнение в ограниченной сессии агента.

- fetch — PASS: недостающие 18 Cargo packages загружены.
- fmt/check/test/Clippy — PASS: 66 Rust-тестов; одна ignored-функция служит
  subprocess fixture и запускается прошедшим тестом отмены процесса.
- layers — PASS: полный разрешённый normal/dev/build/target dependency graph.
- commands/nodes — PASS: 17 команд и 9 определений нод.
- sbt test writeClasspath — PASS: 6 Scala-тестов с настоящим Spark 4.0.1.
- fixtures — PASS: sample/draft/regression/prediction, сравнение таблиц,
  схем и значений с независимым oracle.
- job_lifecycle — PASS: отмена после cleanup JVM, успешный следующий job,
  обнаружение stale результата (snapshot revision 1, current revision 2).
- Итог: `[PASS] requested CI checks completed`.

Ограничения Cargo/sbt-кэша предыдущего агентского запуска не воспроизвелись
в пользовательском терминале. Удалённый GitHub CI и Linux/macOS этим запуском
не проверены. ASSUMPTION: ML-допуск 1e-6 остаётся предварительным acceptance
threshold, не доказанной границей ошибки Spark solver.

## Итерация xtask — предыдущий запуск в ограниченной сессии

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1, JDK 21.0.13:

- fmt/check/test/Clippy — PASS. Workspace: 66 пройденных тестов, включая
  9 новых тестов политики зависимостей и comparator/manifest. Одна ignored-функция
  остаётся subprocess fixture, запускаемой тестом отмены настоящего процесса.
- `cargo xtask commands --offline` — PASS: 17 команд и независимый тест
  соответствия enum/serde/parameter contracts.
- `cargo xtask nodes --offline` — PASS: 9 нод, порты, параметры, defaults,
  typed factories и семь registry contract tests.
- `cargo xtask fixtures --offline --java <JDK21/bin/java.exe>` — PASS:
  sample/draft/regression/prediction выполнены настоящим Spark через Session.
  Схемы, имена таблиц, число строк и значения совпали с отдельным oracle
  `examples/expected/pipelines.json`; draft имеет две ожидаемые readiness issues.
- `cargo xtask layers --offline` — FAIL до анализа: Cargo metadata для всех
  платформ требует отсутствующий в локальном кэше bitflags 2.13.2.
  Повторный запуск без offline также FAIL: static.crates.io недоступен
  (в диагностике, среди прочего, slab 0.4.12). Политика намеренно не заменена
  урезанной проверкой только текущей ОС. Её шесть unit tests прошли, но это
  не подтверждение успешного анализа реального полного workspace graph.
- `cargo xtask ci --offline` — FAIL на layers после успешных Rust checks.
  Полный `ci --with-spark` — NOT_RUN. Для повторения в обычном терминале:
  `cargo fetch --locked`, затем `cargo xtask ci --with-spark`.
- Повторный `sbt test writeClasspath` — FAIL при запуске launcher: нет доступа
  к пользовательскому sbt boot/cache. Scala-тесты в этой итерации — NOT_RUN;
  предыдущие шесть PASS сохраняются как исторический результат. Fixtures
  использовали уже собранный worker и существующий classpath.txt.
- GitHub workflow обновлён: Rust job запускает xtask ci; Spark job после
  sbt проверяет fixtures и сохраняет CLI inspect/lifecycle smoke checks.
  Новый удалённый workflow, Linux/macOS и независимое review — NOT_RUN.

Логи локальные, игнорируются Git: `.tools/xtask-ci.log`, `xtask-layers.log`,
`xtask-fixtures.log`, `xtask-sbt.log`. Layers не использует grep по Cargo.toml:
нужен разрешённый Cargo metadata graph с normal/dev/build/target dependencies.
ASSUMPTION: прежний абсолютный ML-допуск 1e-6 остаётся provisional acceptance
threshold; численная эквивалентность solver не доказана. Fixtures явно печатает
это допущение и не расширяет допуск по результату запуска.

## Итерация фоновых jobs и отмены

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1, JDK 21.0.13:

- cargo fmt --all -- --check — PASS.
- cargo check --workspace --locked --offline — PASS.
- cargo test --workspace --locked --offline — PASS: 57 тестов
  (22 core, 8 project, 23 engine, 4 CLI). Одна дополнительная ignored-функция —
  subprocess fixture с sleep; она запускается отдельным настоящим процессом
  из cancel_terminates_real_child_and_unblocks_reader. Это не пропущенная
  проверка отмены: сам тест завершения процесса и разблокировки reader прошёл.
- cargo clippy --workspace --all-targets --locked --offline -- -D warnings — PASS.
- job_lifecycle example на настоящем JVM — exit 0: отмена после регистрации
  worker, подтверждённый cleanup, следующий successful job, изменение проекта
  во время работы и обнаружение stale результата revision 1 при current 2.
  После финальной правки этапа Executing пример повторно прошёл.
- CLI --timeout-seconds 1 run sample.dcraft — ожидаемый exit 1; диагностирован
  именно TimedOut, а не ошибка аргументов. Runner дождался очистки JVM.
- CLI sample/draft/regression/prediction и inspect sales.csv --rows 3 — exit 0
  через новую фоновую реализацию. Sample/draft: (4,11)/(5,13)/(6,15);
  regression: intercept≈3, coefficient≈2, training RMSE 3.13e-15;
  Predict: 5/7/9/11/13 с округлением; inspect: три строки и исходная схема.
- Дешёвая статика и git diff --check — PASS.

Логи: .tools/jobs-checks/*.stdout.log, *.stderr.log и lifecycle-final.log.
Unit tests контролируют scheduling через каналы/Condvar; synthetic results
используются только для state machine, не как доказательство работы Spark.
Отдельно проверены stop до установки handle, first-stop-wins, Busy, неизвестный
Job ID, terminal events, bounded queue, timeout без polling, panic containment,
best-effort stop при Drop и редактирование Session во время job.

Run теперь возвращает JobStarted; cancel/status зарегистрированы и исполняемы.
Вычисления и supervisor работают в отдельных std threads. Terminal status
публикуется после cleanup. Каждый job создаёт JVM; graceful Spark group cancel,
проценты выполнения и Ctrl+C CLI не реализованы. Новых зависимостей нет.

Scala/protocol не менялись; sbt tests повторно не запускались. Новые CI-шаги,
Linux/macOS и независимое review новой реализации — NOT_RUN. Во время проверки
Clippy потребовал перенести Drop impl перед test module; замечание исправлено.
ASSUMPTION: прежний ML-допуск 1e-6 по-прежнему требует численного обоснования.

## Итерация Session и команд

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1, JDK 21.0.13:

- cargo fmt --all -- --check — PASS.
- cargo check --workspace --locked --offline — PASS.
- cargo test --workspace --locked --offline — PASS: 49 тестов
  (22 core, 8 project, 15 engine, 4 CLI).
- cargo clippy --workspace --all-targets --locked --offline -- -D warnings — PASS.
- CLI commands --json: exit 0; 15 зарегистрированных исполняемых команд,
  без фиктивной execution.cancel. Отдельный CLI-тест использует отсутствующую Java.
- CLI validate sample и validate draft --draft: exit 0; черновик сообщает
  две проблемы незавершённой ноды.
- CLI run draft/sample/regression/prediction и inspect sales.csv --rows 3:
  все exit 0 через Session и настоящий Spark worker. Draft/sample вернули
  (4,11)/(5,13)/(6,15); regression — коэффициенты около 3/2 и RMSE 3.13e-15;
  Predict — ожидаемые 5/7/9/11/13; inspect — три строки и исходную схему.
  Логи: .tools/session-checks/*.stdout.log и *.stderr.log.
- Статическая проверка артефактов и git diff --check — PASS.

Проверены rollback ошибочных команд/Batch, восстановление рёбер/output,
монотонные revisions, no-op/redo, лимит истории, dirty-checkpoints,
ошибки open/save и rebasing current/undo/redo при SaveAs.
Во время разработки исправлены конфликт имени Command со std::process::Command,
неустойчивый порядок рёбер после reconnect и имя CLI-варианта, отклонённое Clippy.
После исправлений команды выше выполнены успешно.

CLI теперь использует Session для open/validate/run/inspect. История содержит
только Project. Runtime config, JVM и ExecutionRecord отделены от .dcraft;
результат помечается revision. Синхронный dispatch Run ещё нельзя отменить
конкурентно; background jobs, progress, timeout и GUI в эту итерацию не входят.

Для тестов engine добавлен tempfile как dev-dependency; пакет уже был в lock,
новые пакеты не загружались. Scala/protocol не менялись, sbt tests не повторялись.
Новые шаги CI не запускались удалённо; новая версия Linux/macOS и независимое
review — NOT_RUN. ASSUMPTION: прежний ML-допуск 1e-6 остаётся необоснованным;
эквивалентность исходным нодам не заявляется.

## Итерация черновых проектов

verified 2026-11-08 via локальные команды Windows, Rust 1.99.1, JDK 21.0.13:

- fmt --check, cargo check --workspace --locked --offline — PASS.
- cargo test --workspace --locked --offline — PASS: 33 теста
  (9 graph, 6 core drafts, 7 registry, 8 project, 3 CLI).
- cargo clippy --workspace --all-targets --locked --offline -- -D warnings — PASS.
- CLI validate draft.dcraft --draft — exit 0, две ожидаемые диагностики
  expression/missing table input на ноде UUID ...0006.
- CLI run draft.dcraft — exit 0, строки (4,11)/(5,13)/(6,15), schema integer.
  Неподключённая нода ...0006 не включена в план исполнения.
- Прежние sample/regression/prediction выполнены через настоящий Spark worker,
  все exit 0; регрессия сохранила коэффициенты около 3/2 и RMSE 3.13e-15.
- Проверки malformed save сохраняют прежние байты файла; NaN/Inf не записываются
  как null. Неполная выбранная ветка отклоняется до попытки запустить Java.
- Дешёвая статическая проверка и git diff --check — PASS.

Логи CLI: .tools/draft-checks/*.stdout.log и *.stderr.log. В первом тестовом
прогоне обнаружена неоднозначность типа пустого Vec в assert_eq; тест исправлен
на is_empty, затем все проверки выше выполнены успешно.
Для CLI-тестов добавлена dev-dependency tempfile, уже используемая project crate;
Cargo.lock обновлён настоящим Cargo offline, новые пакеты не загружались.

Scala worker не менялся, sbt tests повторно не запускались. Linux/macOS и
удалённый CI новой итерации — NOT_RUN. Приёмка независимым reviewer — NOT_RUN.
ASSUMPTION: прежний численный допуск ML-тестов 1e-6 ещё требует обоснования;
эта итерация меняет только правила допуска черновиков и выбора ветки.

## Итерация NodeSpec

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1:

- cargo fmt --all -- --check — PASS.
- cargo check --workspace --locked --offline — PASS.
- cargo test --workspace --locked --offline — PASS: 21 тест
  (9 graph, 7 registry, 4 project, 1 CLI discovery).
- cargo clippy --workspace --all-targets --locked --offline -- -D warnings — PASS.
- Прежние sample/regression/prediction .dcraft выполнены через настоящий
  Rust CLI → Spark worker, все exit 0. Sample дал (4,11)/(5,13)/(6,15);
  regression — intercept 2.9999999999999987, coefficient 2.0000000000000004,
  training RMSE 3.1297001304428877e-15; Predict — 5/7/9/11/13 с округлением.
  Логи: .tools/nodespec-checks/{sample,regression,prediction}.{stdout,stderr}.log.
- CLI nodes --json проверен интеграционным тестом с заведомо отсутствующей
  Java: discovery работает без JVM. В тестах проверены старые поля JSON,
  типы/порты всех девяти нод, defaults, границы и ошибки metadata.
- Дешёвая статическая проверка артефактов — PASS; git diff --check — PASS.

Scala worker и протокол не менялись. sbt test в этой итерации не повторялся:
его предыдущий результат 6/6 сохраняется как историческое свидетельство,
а интеграция после изменения core проверена тремя настоящими CLI-прогонами.
Новый шаг CI nodes --json добавлен, но изменения ещё не отправлены на GitHub:
удалённый CI для этой итерации NOT_RUN. Независимое review и macOS/Linux
прогон новой версии также NOT_RUN. Обоснование прежнего ML-допуска 1e-6
остаётся открытым ASSUMPTION и этим рефакторингом не решается.

## Актуальный baseline после установки инструментов

verified 2026-11-08 via реальные локальные команды: Rust/Cargo 1.99.1,
Windows x86_64 MSVC, Temurin JDK 21.0.13, Spark 4.0.1.

| Проверка | Результат |
|---|---|
| cargo fmt --all -- --check | PASS после применения cargo fmt --all |
| cargo check --workspace --locked --offline | PASS |
| cargo test --workspace --locked --offline | PASS, 9 graph + 4 project tests |
| cargo clippy --workspace --all-targets --locked --offline -- -D warnings | PASS |
| CLI run regression.dcraft | PASS, exit 0 |
| CLI run prediction.dcraft | PASS, exit 0 |
| sbt test writeClasspath | PASS по предоставленному пользователем выводу: 6/6 |
| CLI run sample.dcraft | PASS по предоставленному пользователем выводу: (4,11), (5,13), (6,15) |

Начальный fmt --check обнаружил несоответствие форматированию. Применён
rustfmt; изменения Rust-файлов в этой итерации только форматирующие.
Cargo.lock существует. Для обхода устаревшего PATH использован прямой путь
к установленному toolchain и DATACRAFT_JAVA; пользовательские переменные не менялись.
Предупреждение о canonicalization пользовательского home не помешало командам;
оно не является предупреждением Clippy исходного кода.

ML-примеры запускались настоящим cargo run --locked --offline -p datacraft-cli
с run examples/pipelines/<имя>.dcraft. Фактический вывод регрессии:

```text
Intercept: 2.9999999999999987
feature[0]: 2.0000000000000004
training rmse: 3.1297001304428877e-15
training r2: 1
```

Predict: quantity 1..5, amount 5/7/9/11/13, prediction
4.999999999999999/7/9/11/13. Оракул задаётся независимо: amount = 2*quantity+3.
Логи последних CLI-прогонов: .tools/baseline-checks/{regression,prediction}.{stdout,stderr}.log.
Малые наблюдаемые ошибки не являются выводом допустимой погрешности solver.

NOT_RUN: отдельная приёмка Linux/macOS, новый запуск удалённого CI,
полная проверка внешних фактов зависимостей, независимое поведенческое review.
ASSUMPTION: порог 1e-6 ML-тестов ещё не обоснован анализом solver;
эквивалентность старым нодам не заявляется. Отдельные команды CLI validate и
inspect в этой итерации не запускались.

Далее сохранена история первоначальных ограничений окружения; её статусы
не заменяют актуальную таблицу выше.

## Обновление: исправлен Windows Parquet fixture

Ниже сохранён первоначальный отчёт окружения; он не описывает все последующие
изменения установки инструментов. Rust 1.99.1, Cargo, rustfmt, Clippy и JDK
21.0.13 впоследствии найдены; минимальная Rust-программа собрана и запущена.
Это не означает прохождения cargo test всего workspace.

Пользователь запустил sbt 1.10.11: WorkerSuite первоначально дал 5 PASS / 1 FAIL.
XML-отчёт показал ошибку Hadoop Windows permissions при записи Parquet fixture.
Подготовка fixture заменена на Parquet LocalOutputFile через Java NIO.
Исправленный тест скомпилирован Scala 2.13.16; JUnitCore выполнил все шесть
тестов WorkerSuite с настоящим Spark 4.0.1: OK (6 tests), exit 0.
Подробности и ограничения: [PARQUET_SOURCE_TEST.md](PARQUET_SOURCE_TEST.md).
Повторный sbt test writeClasspath и Rust–worker CLI этим прогоном не проверены.

## Выполнено успешно

- Python `tomllib` прочитал все пять `Cargo.toml` без ошибок синтаксиса TOML.
- Python `json` прочитал три примера `.dcraft`; проверены уникальность UUID,
  наличие designated output, порядок зависимостей и существование CSV по
  относительному пути каждого проекта.
- Проверен CSV: 12 строк, `amount = 2 * quantity + 3`; ожидаемый результат
  фильтра `quantity >= 4` с limit 3 — quantity 4, 5, 6.
  Это проверка исходных fixtures на Python, не исполнение Spark.
- Найден и запущен Java runtime:
  `C:\Program Files\JetBrains\CLion 2025.1.1\jbr\bin\java.exe` — OpenJDK 21.0.6.

## Не удалось запустить

| Команда | Результат |
|---|---|
| `cargo fmt --all -- --check` | `cargo` не найден |
| `cargo check --workspace` | `cargo` не найден |
| `cargo test --workspace` | `cargo` не найден |
| `cargo clippy --workspace --all-targets -- -D warnings` | `cargo` не найден |
| `sbt test` | `sbt` не найден |

Ни одна проверка компиляции Rust/Scala не отмечается как пройденная.
Форматирование Rust ещё требует запуска rustfmt. Cargo.lock отсутствует.
Команды `datacraft validate/run/inspect` и реальный Spark pipeline не запускались:
для них сначала нужны собранные CLI и worker.

## Блокеры окружения

- Rust/rustup не найдены в PATH и стандартных пользовательских каталогах.
- sbt и Spark не найдены в PATH.
- Загрузка Rust manifest через PowerShell не установила соединение.
- Запрос Maven через Python завершился `WinError 10013`: доступ к сокету запрещён.
- `wsl --list --quiet` завершился `Wsl/EnumDistros/Service/E_ACCESSDENIED`.

Привилегии не повышались, системные настройки не менялись.

## Подготовленные, но не выполненные тесты

- Rust core: DAG, циклы, отсутствующие ссылки, порты, несовместимые типы,
  обязательные входы, повторные соединения/UUID, параметры, выбор выхода,
  замыкание зависимостей и детерминированный порядок.
- Rust project: JSON round-trip, канонический порядок, версии, повреждённый
  JSON, сохранение/перезапись и защита будущих версий, относительные пути.
- Scala: CSV pipeline с точными строками и схемой, коэффициенты 3 и 2,
  обучающие метрики, Predict, ошибки колонок/файлов/фильтра, Parquet.
- CI: Rust fmt/check/test/Clippy, Scala tests и сквозные CLI-команды.

CI только описан в файле workflow; удалённый запуск не производился.
