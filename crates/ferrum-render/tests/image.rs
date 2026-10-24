use ferrum_render::{decode_png, Pixmap, MAX_IMAGE_SIDE};

/// Заголовок PNG с IHDR, но без данных: достаточно, чтобы проверка размера
/// отработала до декодера.
fn png_header(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    // глубина 8 бит, RGBA, сжатие/фильтр/чересстрочность по нулям
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v.extend_from_slice(&[0, 0, 0, 0]); // CRC (неверный — тест не до него доходит)
    v
}

/// Маленький валидный PNG 3×2 с известными пикселями.
fn small_png() -> Vec<u8> {
    let mut pm = Pixmap::new(3, 2);
    let px = pm.data_as_u8_slice_mut();
    for (i, chunk) in px.chunks_exact_mut(4).enumerate() {
        // непрозрачные пиксели: премультипликация на них не меняет значения
        chunk.copy_from_slice(&[(i * 40) as u8, 100, 200 - (i * 20) as u8, 255]);
    }
    pm.into_png().expect("кодирование PNG")
}

#[test]
fn garbage_bytes_are_rejected() {
    assert!(decode_png(&[]).is_none());
    assert!(decode_png(b"not a png at all, just some ordinary bytes here").is_none());
    // Правильная сигнатура, но заголовок короче 24 байт.
    assert!(decode_png(b"\x89PNG\r\n\x1a\nIHDR").is_none());
}

#[test]
fn truncated_png_is_rejected() {
    let full = small_png();
    assert!(decode_png(&full).is_some());
    assert!(decode_png(&full[..full.len() / 2]).is_none());
    // Обрыв внутри данных изображения (IEND и CRC последнего IDAT — 20 байт).
    assert!(decode_png(&full[..full.len() - 20]).is_none());
    // Без одного только CRC в конце картинка полная: декодер её принимает.
    assert!(decode_png(&full[..full.len() - 4]).is_some());
}

#[test]
fn oversized_header_is_rejected_before_decoding() {
    // 65535×65535 в заголовке: декодер не должен выделять ~16 ГБ.
    assert!(decode_png(&png_header(65535, 65535)).is_none());
    assert!(decode_png(&png_header(MAX_IMAGE_SIDE as u32 + 1, 1)).is_none());
    assert!(decode_png(&png_header(1, MAX_IMAGE_SIDE as u32 + 1)).is_none());
}

#[test]
fn zero_dimensions_are_rejected() {
    assert!(decode_png(&png_header(0, 10)).is_none());
    assert!(decode_png(&png_header(10, 0)).is_none());
    assert!(decode_png(&png_header(0, 0)).is_none());
}

#[test]
fn patched_oversized_width_in_real_png_is_rejected() {
    // Настоящий PNG с шириной, переписанной на 65535 (CRC уже не сходится).
    let mut bytes = small_png();
    bytes[16..20].copy_from_slice(&65535u32.to_be_bytes());
    assert!(decode_png(&bytes).is_none());
}

#[test]
fn valid_small_png_round_trips() {
    let bytes = small_png();
    let pm = decode_png(&bytes).expect("валидный PNG декодируется");
    assert_eq!((pm.width(), pm.height()), (3, 2));
    for i in 0..6u16 {
        let p = pm.sample(i % 3, i / 3);
        assert_eq!(
            (p.r, p.g, p.b, p.a),
            ((i * 40) as u8, 100, 200 - (i * 20) as u8, 255),
            "pixel {i}"
        );
    }
}
