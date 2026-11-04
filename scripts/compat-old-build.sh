#!/bin/sh
# Совместимость (спека подсистемы 3, разделы 3, 9, 10 и 12): файл с узлом нового вида,
# собранный новой сборкой, открывается сборкой без этого вида только для чтения:
# прямоугольник виден, новый узел — узел неизвестного типа — скрыт, правка
# отвергается. Старая сборка собирается во временной копии репозитория.
#   scripts/compat-old-build.sh [ref] [fixture]
# fixture — пример `ferrum-doc`: `text_fixture` (по умолчанию; ref — сборка без
# текста, 2710c85), `vector_fixture` (ref — сборка без векторов, например main
# до этапа 3) или `boolean_fixture` (ref — сборка без булевых групп, например main
# до этапа 4: группа скрыта вместе с детьми). `guides_fixture` (ref — сборка без
# направляющих, например main до этапа 4 подсистемы 2) — другое обещание: файл
# открывается как обычно, для записи (неизвестный ключ просто не читается), а
# пересохранённый старой сборкой не теряет направляющих — это проверяет новая сборка.
# Старая сборка идёт в свой каталог `$CARGO_TARGET_DIR/compat-old`: у членов workspace
# хэш артефакта не зависит от пути, и в общем каталоге старый `ferrum-doc` перезаписал
# бы отладочную библиотеку новой сборки.
set -eu
REF="${1:-2710c85}"
FIXTURE="${2:-text_fixture}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
FILE="$TMP/fixture.ferrum"
cleanup() {
  git -C "$ROOT" worktree remove --force "$TMP/old" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT
(cd "$ROOT" && cargo run -q -p ferrum-doc --example "$FIXTURE" -- "$FILE")
git -C "$ROOT" worktree add --detach "$TMP/old" "$REF" >/dev/null
if [ "$FIXTURE" = guides_fixture ]; then
  cat > "$TMP/old/crates/ferrum-doc/tests/compat_fixture.rs" <<'RS'
use ferrum_doc::container::{decode, encode};
use ferrum_doc::NodeKind;
#[test]
fn file_with_guides_opens_as_usual() {
    let bytes = std::fs::read(std::env::var("FERRUM_FIXTURE_FILE").unwrap()).unwrap();
    let opened = decode(&bytes).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    let mut doc = opened.document;
    let page = doc.pages()[0].0;
    assert_eq!(doc.page(page).unwrap().walk().len(), 2, "фрейм и прямоугольник");
    assert!(!doc.is_read_only(), "неизвестный ключ не мешает правке");
    doc.transact(page, vec![], |tx| Ok(vec![tx.create_node(None, None, NodeKind::Ellipse)?]))
        .unwrap();
    std::fs::write(std::env::var("FERRUM_FIXTURE_OUT").unwrap(), encode(&doc).unwrap()).unwrap();
}
RS
  (cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/compat-old" FERRUM_FIXTURE_FILE="$FILE" \
    FERRUM_FIXTURE_OUT="$TMP/resaved.ferrum" cargo test -q -p ferrum-doc --test compat_fixture)
  (cd "$ROOT" && cargo run -q -p ferrum-doc --example guides_fixture -- --check "$TMP/resaved.ferrum")
  echo "Совместимость с $REF ($FIXTURE): файл открыт как обычно, направляющие пережили пересохранение"
  exit 0
fi
cat > "$TMP/old/crates/ferrum-doc/tests/compat_fixture.rs" <<'RS'
use ferrum_doc::container::decode;
use ferrum_doc::{DocError, NodeKind};

#[test]
fn file_with_new_kind_opens_read_only() {
    let bytes = std::fs::read(std::env::var("FERRUM_FIXTURE_FILE").unwrap()).unwrap();
    let mut doc = decode(&bytes).unwrap().document;
    let page = doc.pages()[0].0;
    let walk = doc.page(page).unwrap().walk();
    assert_eq!(walk.len(), 1, "виден только прямоугольник");
    assert!(doc.is_read_only(), "новый узел скрыт — документ только для чтения");
    let err = doc.transact(page, vec![], |tx| Ok(vec![tx.create_node(None, None, NodeKind::Rect)?]));
    assert!(matches!(err, Err(DocError::ReadOnly)));
}
RS
(cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/compat-old" FERRUM_FIXTURE_FILE="$FILE" \
  cargo test -q -p ferrum-doc --test compat_fixture)
echo "Совместимость с $REF ($FIXTURE): файл открыт только для чтения"
