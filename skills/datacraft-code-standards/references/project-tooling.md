# Rust/Scala вместо Python project wrapper

## Соответствие инструментов

| Исходный стандарт | DataCraft |
|---|---|
| requirements, wheels, pip | Cargo.toml/Cargo.lock; build.sbt и Maven artifacts |
| venv и Python interpreter | Rust toolchain, linker/SDK, JDK и sbt |
| pytest | Rust tests и Scala MUnit; реальный CLI как consumer |
| Python AST/ruff | rustfmt + cargo check/Clippy; sbt compile/test |
| verify_install.py | версии инструментов и smoke pipeline через реальные команды |
| REPRODUCIBILITY | версии, платформы, данные, параметры и команды |

Не создавайте setup/download wrappers без необходимости. Если они нужны,
обеспечьте повторный запуск, явную offline-политику, корректные exit codes
и полезный stderr. Отделяйте диагностику от установки; не устанавливайте
инструменты глобально молча.

## Внешние факты

Для каждой прямой зависимости проверяйте опубликованную версию, фактический
API, лицензию и platform requirements по официальному источнику. Для Rust
учитывайте features, MSRV и native dependencies; для Spark — Scala binary
suffix, JDK, ML API и транзитивные native runtime libraries.

Разрешённая версия не означает успешную сборку. Успешная Linux-сборка не
доказывает Windows/macOS. Доступный URL не доказывает наличие нужного ассета,
а записанный локально SHA256 не подтверждает происхождение загрузки.
Для загрузок проверяйте опубликованные checksum/signature, размер и содержимое;
частичный существующий файл не считается завершённой установкой.

Формат реестра: утверждение → источник/команда → дата → наблюдение → область
проверки → открытое допущение. «Java запустилась» и «Spark tests прошли с этой
Java» — отдельные записи. Реестр может жить в `docs/VALIDATION.md` или
`docs/REPRODUCIBILITY.md`. Используйте фактическую дату; не переносите verified
из другого проекта. Непроверенное помечайте `ASSUMPTION:`.

## Воспроизводимость

- Генерируйте Cargo.lock настоящим Cargo и сохраняйте для приложения. После
  появления lock проверяйте release/CI с --locked; не сочиняйте lock вручную.
- Фиксируйте Scala, Spark и sbt в файлах сборки. При смене версии сохраняйте
  отчёт разрешения зависимостей и проверяйте runtime classpath.
- Фиксируйте проверенный Rust toolchain/JDK, ОС, архитектуру, build profile,
  JVM options и relevant environment. Stable не является неизменяемым pin.
- Для ML фиксируйте данные/хэши, schema, seeds, solver, параметры, порядок
  признаков, preprocessing и число partitions/threads там, где это значимо.
- Относительные dataset paths разрешайте от проекта, не от CWD. Classpath
  генерируйте заново после переноса каталога или смены ОС.
- Rust/Scala identifiers и CLI — на английском; README и архитектурные
  объяснения — на русском. Различайте implemented и planned.

## Установка и платформы

Проверяйте Windows MSVC + C++ tools/SDK отдельно от Linux linker/toolchain
и macOS Command Line Tools. Версия JDK должна соответствовать проверенной
документации выбранного Spark; не переносите JDK по привычке.
Поддержка ОС — статус проверки, а не только архитектурная цель. Не вводите
Docker/WSL или Python runtime как обязательную зависимость нативного приложения
без технической причины.
