# 🦀 Bytekodex Platform

<p align="center">
  <a href="https://github.com/bytekodex/bytekodex-platform/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/bytekodex/bytekodex-platform?label=release"></a>
  <a href="https://github.com/bytekodex/bytekodex-platform/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/bytekodex/bytekodex-platform/ci.yml?branch=master&label=ci"></a>
  <img alt="Rust 1.100" src="https://img.shields.io/badge/Rust-1.100-CE422B?logo=rust&logoColor=white">
  <img alt="C ABI" src="https://img.shields.io/badge/C%20ABI-v1-555555">
  <img alt="Linux | macOS | Windows" src="https://img.shields.io/badge/Linux%20%7C%20macOS%20%7C%20Windows-x64%20%7C%20arm64-2F6FEB">
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/bytekodex/bytekodex-platform"></a>
</p>

🦀 **Bytekodex Platform** читает скомпилированный байткод и рисует его. Она сама разбирает class-файлы JVM, превращает их в платформенно-независимый поток токенов и рисует этот поток в индексированный PNG. Всё это на Rust за небольшим C ABI. На ней построен [@bytekodex_bot](https://github.com/bytekodex/bytekodex-telegram).

[English version](README.md)

<p align="center"><img src="https://raw.githubusercontent.com/bytekodex/bytekodex-telegram/master/docs/java-switch.png" width="760" alt="Байткод switch по sealed-интерфейсу, нарисованный платформой: методы, смещения и опкоды в цвете"></p>

## С чего начать

Скачайте архив для своей платформы из [последнего релиза](https://github.com/bytekodex/bytekodex-platform/releases/latest): Linux (x64 и arm64), macOS на Apple silicon или Windows (x64). В каждом лежат динамическая и статическая библиотеки, [`bytekodex.h`](include/bytekodex.h) и `bk` — утилита командной строки, чтобы попробовать:

```shell
bk dump --pool --locals Foo.class
bk render Foo.class --font /path/to/JetBrainsMono-Regular.ttf --size 26 --rows 34 --out page
```

`--font` принимает только одиночный шрифт TrueType. Коллекция `.ttc` им не является, и `bk` её отклонит.

## Что изменилось по сравнению с painter

Платформа заменяет [bytekodex-painter](https://github.com/bytekodex/bytekodex-painter) и [bytekodex-antlr](https://github.com/bytekodex/bytekodex-antlr), оба теперь в архиве.

**Без `javap`.** Class-файл разбирается напрямую. Вызов `javap` стоил запуска JVM на каждый запрос, это десятки миллисекунд и десятки мегабайт, ради текста, который потом приходилось снова разбирать на токены. Чтение бинарного формата убирает обе половины: оно занимает микросекунды, не требует JDK и сразу выдаёт токены, которые знают, чем являются.

**Без ANTLR.** Когда есть бинарный читатель, на быстром пути распознавать нечего, и грамматике нечего делать. Лексер на `logos` остался для единственного случая, где он нужен: пользователь вставляет в бота вывод `javap`.

**Без 2D-графики.** Байткод — это моноширинный текст на ровном фоне. Позиция считается как `column * advance`, глифы растеризуются один раз в кэш, а единственная фигура — четыре скруглённых угла. Skia тащила целый конвейер текста и векторов ради работы, о которой мы её не просили.

**Индексированный вывод.** Дамп использует около двадцати цветов, поэтому кадр хранит один байт на пиксель: какой цвет и насколько он покрывает пиксель. Смешивание делает палитра PNG. Вчетверо меньше памяти и намного меньше файл.

## Устройство

| Крейт | Зависит от | Роль |
| --- | --- | --- |
| `bk-core` | ничего | IR токенов, документ, статистика, ошибки. Контракт для всех остальных крейтов |
| `bk-theme` | core | Цвет для каждого вида токена |
| `bk-jvm` | core, logos | Чтение class-файлов, таблица опкодов, лексер текста javap |
| `bk-render` | core, theme, fontdue, png | Рисование моноширинной сетки, кодировщик индексированного PNG |
| `bk-ffi` | core, jvm, render, theme | C ABI: `cdylib` и `staticlib` |
| `bk-cli` | core, jvm, render, theme, rayon | `bk` для локальной работы |

Новый формат байткода — это новый крейт, реализующий `bk_core::Frontend`. В `bk-render` ничего не знает об опкодах.

## Сборка

Rust 1.100 закреплён в [`rust-toolchain.toml`](rust-toolchain.toml), rustup скачает его при первом вызове `cargo`.

```shell
cargo build --release          # target/release/libbytekodex.{so,dylib,dll} и libbytekodex.a
cargo test --workspace
cargo run --bin bk -- dump --pool --locals Foo.class
```

## C ABI

[`include/bytekodex.h`](include/bytekodex.h) — это спецификация. Он написан вручную, чтобы каждое изменение было осознанным. Вкратце:

```c
bk_renderer *r = bk_renderer_new(font, 40.0f, &status);

bk_response response;
int32_t status = bk_render(r, &request, buffer, capacity, &response);
if (status == BK_ERR_BUFFER_TOO_SMALL) {
    /* response.required is exact: grow and retry */
}
```

Выходной буфер принадлежит вызывающей стороне. Освобождать нужно только сам рендерер, так что аллокатор не пересекает границу и забыть про утечку нельзя. Счётчики попадают в `response` даже при ошибке рисования, поэтому сторона, упёршаяся в лимит размера, всё равно может сообщить, сколько опкодов она увидела.

## О производительности

Class-файлы весят от одного до пятидесяти килобайт. На таком размере `read` в переиспользуемый буфер быстрее `mmap`: его page fault и освобождение стоят дороже копирования. Поэтому `memmap2` здесь не зависимость, его место за флагом для jar-файлов.

Опкоды считает обычный локальный счётчик. Атомарный инкремент на каждую инструкцию занял бы большую часть разбора большого метода и ничего бы не дал: каждый поток владеет своим классом, а `rayon` сводит результаты в конце. Единственный атомик в коде считает опкоды на весь процесс, трогается раз за запрос и читается только метриками.

## Проверено на

997 class-файлах из дистрибутива ANTLR (версии 49.0, 50.0 и 51.0): ни одной испорченной инструкции и ни одного неизвестного опкода.

## Релизы

Что приносит каждая версия, написано в [CHANGELOG.md](CHANGELOG.md). Релиз — это ручной запуск workflow «Release» с новой версией, а каждый понедельник автопилот обновляет зависимости и выпускает то, что изменилось.

## Связанные проекты

- **[@bytekodex_bot](https://github.com/bytekodex/bytekodex-telegram)** — Telegram-бот на этой библиотеке: пришлите Java, Kotlin или Groovy и получите картинку байткода.
- **[Рабочее пространство Bytekodex](https://github.com/bytekodex/.github)** — все репозитории Bytekodex одним клоном, как сабмодули.
- **[bytekodex-painter](https://github.com/bytekodex/bytekodex-painter)** — исходный рисовальщик на ANTLR и Skia, который заменила эта библиотека, в архиве.
- **[bytekodex-antlr](https://github.com/bytekodex/bytekodex-antlr)** — грамматики javap для него, в архиве.
- **[Capsa](https://github.com/MairwunNx/Capsa)** — движок Kotlin Multiplatform для экранов из маленьких возможностей.
- **[WorldAPI](https://github.com/MairwunNx/minecraft-worldapi)** — сервер Minecraft по HTTP, для сайтов, ботов и ИИ-агентов.

> 🫡 Сделал Павел Ерохин (Pavel Erokhin), aka mairwunnx.
