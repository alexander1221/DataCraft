# Проверки первой реализации

Дата: 2026-11-08. Среда: Windows, PowerShell, Python 3.14.3.

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
