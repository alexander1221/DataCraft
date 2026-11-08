# Repository Guidelines

## Структура и архитектура

DataCraft — локальное приложение: Rust workspace, desktop на eframe/egui,
Scala worker с Spark SQL/ML. DataFusion пока не реализован.

- `crates/datacraft-core/`: UUID, DAG, валидация, реестр NodeSpec.
- `crates/datacraft-project/`: версионированные `.dcraft`, сохранение и пути.
- `crates/datacraft-engine/`: headless Session, команды, undo/redo, jobs и кэш.
- `apps/datacraft-cli/`, `apps/datacraft-desktop/`: клиентские адаптеры.
- `workers/spark/`: Scala-код и MUnit-тесты в `src/main/scala/` и `src/test/scala/`.
- `xtask/`: проверки архитектуры; `examples/`: данные, pipeline и независимые
  эталоны в `expected/`; `docs/`: контракты и результаты проверок.

Все правки проекта проводите через команды Session. Метаданные нод определяйте
в центральном реестре. Разделяйте Project, execution state и UI state;
сохраняйте UUID независимо от canvas. Core/project не зависят от GUI или Spark.
Не блокируйте UI запросами worker. Границы проверяет `cargo xtask layers`.

## Сборка и запуск

Нужны Rust stable с rustfmt/Clippy, JDK 17/21 и sbt; Windows/MSVC требует
C++ Build Tools и Windows SDK. Версии закреплены в Cargo.lock и workers/spark:
Scala 2.13.16, Spark 4.0.1. Из корня:

```text
cargo xtask ci
cargo xtask ci --with-spark
cargo xtask fixtures
cargo run -p datacraft-cli -- validate examples/pipelines/sample.dcraft
cargo run -p datacraft-desktop -- examples/pipelines/sample.dcraft
```

`ci` выполняет fmt/check/test/Clippy и проверки реестров/слоёв. `--with-spark`
добавляет сборку и тесты worker, fixtures и проверку отмены/восстановления JVM.
`fixtures` требует собранного worker: выполните `sbt test writeClasspath`
из `workers/spark`. Установку системных библиотек Linux смотрите в README.

## Стиль и стандарты генерации

Перед реализацией прочитайте [SKILL.md](skills/datacraft-code-standards/SKILL.md)
и применимые references. Для нетривиальных модулей сначала задайте контракт
§1–§4, затем дополните §5–§8. Следуйте rustfmt: Rust — четыре пробела,
Scala — два; функции Rust именуйте snake_case, типы — PascalCase.
Документация — по-русски, identifiers и CLI — по-английски.
Используйте структурированные ошибки; не заменяйте runtime-валидацию panic/unwrap.

## Тестирование

Rust-тесты размещайте в модулях и `<crate>/tests/`; Scala использует MUnit.
Называйте тесты по проверяемому поведению. Общий порог покрытия не установлен.
Проверяйте ошибки, инвалидацию кэша, совместимость проектов и ограниченные preview.
Ожидания выводите независимо, численные допуски обосновывайте. GUI/Spark-приёмка
описана в docs/DESKTOP.md и docs/NODE_EXECUTION.md. Записывайте PASS, FAIL и
NOT_RUN в docs/VALIDATION.md; проверка JSON не заменяет компиляцию.

## Коммиты, PR и безопасность

История использует короткие английские imperative subjects: `Add ...`, `Fix ...`.
В PR укажите проблему, изменение поведения, проверки, связанные issues;
для UI приложите screenshots и результаты нативного запуска.
Перед коммитом проверяйте `git diff --cached --check` и состав изменений.

Не коммитьте credentials, личные datasets, target, classpath, кэши и логи.
Сохраняйте совместимость `.dcraft`; не перезаписывайте неизвестные версии.
Не добавляйте telemetry или выполнение shell/scripts через ноды.
Не удаляйте посторонние файлы; push и публикация требуют явного запроса.
