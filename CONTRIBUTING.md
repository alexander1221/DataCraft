# Участие в разработке DataCraft

DataCraft находится на этапе headless Rust CLI и локального Scala/Spark worker.
Подтверждённый baseline — Windows x64/JDK 21; Linux CI подготовлен, но его
удалённый запуск ещё не подтверждён. GUI и DataFusion пока не реализованы.

## Окружение

Установите Rust stable с rustfmt/Clippy, JDK 17 или 21 и sbt.
На Windows для MSVC нужны C++ Build Tools и Windows SDK.
Версии Scala/Spark/sbt задаются в workers/spark; Cargo.lock сохраняется.
Сборка worker: `sbt test writeClasspath` из `workers/spark`.
Полные команды запуска находятся в [README.md](README.md).

## Проверки изменений

Из корня workspace:

```sh
cargo xtask ci
cargo xtask fixtures
```

Для полного прогона с пересборкой worker: `cargo xtask ci --with-spark`.
Отдельные проверки: `cargo xtask layers`, `commands`, `nodes`. Каждый workspace
crate обязан объявить `[package.metadata.datacraft] layer = "..."`; политика,
включая dev/build/target зависимости, описана в
[ARCHITECTURE_CHECKS.md](docs/ARCHITECTURE_CHECKS.md).

При изменении worker выполните также `sbt test writeClasspath`. Не отмечайте
неисполненные проверки как пройденные. Добавляйте тесты поведения и граничных
случаев; ожидаемые значения должны иметь независимый оракул. Численные допуски
требуют обоснования; текущее ограничение ML-тестов описано в docs/VALIDATION.md.

## Границы и стиль

Граф и сохранённые проекты не зависят от GUI или JVM runtime. Не переносите
datasets в Rust ради отображения: preview должен быть ограничен. Изменения
формата .dcraft должны явно учитывать версию и совместимость старых файлов.
Не добавляйте shell/script execution в ноды.

Используйте rustfmt, описательные identifiers и структурированные ошибки.
Пользовательская документация — на русском, identifiers и CLI — на английском.
Для работы агента применяйте
[стандарты генерации](skills/datacraft-code-standards/SKILL.md).
Папка skills требует явного подключения по пути; это не автоматическая установка.
Исходный ZIP стандартов остаётся локальным и не нужен для сборки.

## Коммиты и pull requests

Используйте короткий imperative subject, например `Add typed node registry`.
В PR опишите проблему, поведение после изменения и выполненные проверки.
Для видимых UI-изменений приложите снимки экрана, когда появится desktop.
Укажите связанные issues, новые зависимости и известные ограничения.

Не включайте target, абсолютный classpath JVM, кэши, логи, credentials или
персональные datasets. Перед коммитом проверяйте `git diff --cached --check`
и состав `git diff --cached --stat`.

Вклад в код предоставляется на условиях MIT OR Apache-2.0, если явно не
согласовано иное. Сохраняйте необходимые notices внешних компонентов.
