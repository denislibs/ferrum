#!/bin/sh
# Совместимость (спека подсистемы 3, раздел 12): файл с текстом, собранный новой
# сборкой, открывается сборкой без текста (по умолчанию — `main`) только для
# чтения: прямоугольник виден, текст — узел неизвестного типа — скрыт, правка
# отвергается. Старая сборка собирается во временной копии репозитория.
#   scripts/compat-old-build.sh [ref]
set -eu
REF="${1:-main}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
FILE="$TMP/text.ferrum"
cleanup() {
  git -C "$ROOT" worktree remove --force "$TMP/old" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT
(cd "$ROOT" && cargo run -q -p ferrum-doc --example text_fixture -- "$FILE")
git -C "$ROOT" worktree add --detach "$TMP/old" "$REF" >/dev/null
cat > "$TMP/old/crates/ferrum-doc/tests/compat_text.rs" <<'RS'
use ferrum_doc::container::decode;
use ferrum_doc::{DocError, NodeKind};

#[test]
fn file_with_text_opens_read_only() {
    let bytes = std::fs::read(std::env::var("FERRUM_TEXT_FILE").unwrap()).unwrap();
    let mut doc = decode(&bytes).unwrap().document;
    let page = doc.pages()[0].0;
    let walk = doc.page(page).unwrap().walk();
    assert_eq!(walk.len(), 1, "виден только прямоугольник");
    assert!(doc.is_read_only(), "текст скрыт — документ только для чтения");
    let err = doc.transact(page, vec![], |tx| Ok(vec![tx.create_node(None, None, NodeKind::Rect)?]));
    assert!(matches!(err, Err(DocError::ReadOnly)));
}
RS
(cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}" FERRUM_TEXT_FILE="$FILE" \
  cargo test -q -p ferrum-doc --test compat_text)
echo "Совместимость с $REF: файл с текстом открыт только для чтения"
