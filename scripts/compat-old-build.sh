#!/bin/sh
# Совместимость (спека подсистемы 3, разделы 3, 9 и 12): файл с узлом нового вида,
# собранный новой сборкой, открывается сборкой без этого вида только для чтения:
# прямоугольник виден, новый узел — узел неизвестного типа — скрыт, правка
# отвергается. Старая сборка собирается во временной копии репозитория.
#   scripts/compat-old-build.sh [ref] [fixture]
# fixture — пример `ferrum-doc`: `text_fixture` (по умолчанию; ref — сборка без
# текста, 2710c85) или `vector_fixture` (ref — сборка без векторов, например main
# до этапа 3).
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
(cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}" FERRUM_FIXTURE_FILE="$FILE" \
  cargo test -q -p ferrum-doc --test compat_fixture)
echo "Совместимость с $REF ($FIXTURE): файл открыт только для чтения"
