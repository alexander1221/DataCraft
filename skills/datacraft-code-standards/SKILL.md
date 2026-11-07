---
name: datacraft-code-standards
description: Apply the user's generation and verification standards when writing or reviewing DataCraft Rust, Scala/Spark, project formats, development scripts, tests, or release packages. Use for DataCraft implementation and delivery; loading this skill does not certify existing code.
---

# Стандарты генерации DataCraft

Адаптация пользовательского `code-generation-standards_v4.zip` для этого
репозитория. Исходный архив сохраняется как источник; его неподтверждённые
внешние утверждения не становятся проверенными фактами после переноса.

## Постоянные требования

1. До нетривиальной реализации опишите задачу, алгоритм, применения и контракт
   (§1–§4). Дополните реализацией, замечаниями, проверенными примерами и анализом
   ошибок (§5–§8); для нечисленного кода используйте §8′ «Устойчивость».
   Документируйте связанную процедуру или модуль, не копируйте восемь секций
   в каждый getter. См. [documentation.md](references/documentation.md).
2. Ожидания тестов выводите независимо от проверяемого кода. Не превращайте
   собственный вывод в эталон. Обосновывайте численные допуски до подгонки тестов.
3. Разделяйте существование зависимости, разрешение версии, компиляцию,
   запуск на целевой платформе и проверку у потребителя. Внешние утверждения
   фиксируйте как `verified <date> via <source>` с границами проверки либо
   `ASSUMPTION: <что и как проверить>`. Общий реестр допустим вместо повторения
   маркера у каждого упоминания версии.
4. Для численной переписки указывайте эталон и уровень `bitwise`, `ulp(k)` или
   `bound(B)`. Новая Spark ML интеграция сама по себе не доказывает эквивалентность
   старым нодам. См. [numerics.md](references/numerics.md).
5. Невалидный пользовательский ввод возвращает `Result` в Rust, структурированную
   ошибку в Scala worker, исключение во вспомогательных Python-скриптах.
   `assert` допустим в тестах; `panic!`, `unwrap`, `expect` и Scala `assert`
   не заменяют runtime-валидацию недоверенного ввода.
6. После массовых изменений запустите парсер/компилятор соответствующего языка.
   Отсутствующий инструмент — `NOT_RUN`, а не успех. JSON/TOML-проверка не
   является проверкой Rust/Scala. Не подавляйте предупреждения ради зелёного CI.
7. Соблюдайте Windows-границы: ASCII для `.ps1/.bat/.cmd`, UTF-8 для исходников,
   Markdown и JSON-протокола; явная кодировка на границах процессов.
   См. [windows-packaging.md](references/windows-packaging.md).
8. Перед сдачей выполните применимые ворота из
   [verification.md](references/verification.md); перечислите открытые допущения.
   Файлы или подготовленный workflow не доказывают рабочее исполнение.

## Что читать под задачу

| Работа | Reference |
|---|---|
| Новый модуль, контракт ноды, протокол | [documentation.md](references/documentation.md) |
| Workspace, зависимости, setup, воспроизводимость | [project-tooling.md](references/project-tooling.md) |
| Численная миграция, метрики, preprocessing | [numerics.md](references/numerics.md) |
| PowerShell, Unicode, пути, ZIP | [windows-packaging.md](references/windows-packaging.md) |
| Тесты, CI, приёмка, итоговый отчёт | [verification.md](references/verification.md) |

Читайте только применимые references. Python runtime не нужен приложению:
Python 3.11+ используется лишь необязательным developer-валидатором.

## Проверки

Из корня репозитория:

```text
python skills/datacraft-code-standards/scripts/validate_generated.py .
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

В `workers/spark`: `sbt test writeClasspath`; затем реальные CLI-примеры.
Путь к helper изменится при переносе skill folder; используйте его новое
местоположение. Самотестирование: `python <validator-path> --selftest`.
Валидатор проверяет имена, Python syntax, JSON/.dcraft, TOML и ASCII скриптов.
Он не проверяет Rust/Scala, YAML, зависимости и сеть; явно сообщает об этом.

Не запускайте старый `check_external.py` с Python platform tags как доказательство
совместимости Cargo/Maven. Не добавляйте numpy, pytest, venv или Python setup
только потому, что они были в исходном навыке.

## Область применения

Работайте в авторизованном объёме. Адаптация стандарта не разрешает менять
системные настройки, публиковать релиз или автоматически переписывать весь
существующий код. Независимую проверку высокорисковых изменений выполняйте
человеком либо отдельно авторизованным агентом; независимая формула может
служить оракулом, но не подменяет тест у настоящего потребителя.
