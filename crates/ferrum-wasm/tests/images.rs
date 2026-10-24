//! Картинки документа в worker'е: декодирование по запросу с бюджетом на кадр,
//! постоянная заглушка для неразборчивых и предел памяти.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_doc::ImageStore;
use ferrum_render::{ImageProvider, Pixmap};
use ferrum_wasm::images::{DocImages, DECODES_PER_FRAME};

fn png(w: u16, h: u16, rgba: [u8; 4]) -> Vec<u8> {
    let mut p = Pixmap::new(w, h);
    for px in p.data_as_u8_slice_mut().chunks_exact_mut(4) {
        px.copy_from_slice(&rgba);
    }
    p.into_png().unwrap()
}

#[test]
fn decodes_at_most_a_few_images_per_frame() {
    let mut store = ImageStore::default();
    let hashes: Vec<String> = (0..5)
        .map(|i| store.add(png(4, 4, [i * 40, 0, 0, 255])))
        .collect();
    let images = DocImages::default();
    images.begin_frame();
    let p = images.provider(&store);
    let got: Vec<bool> = hashes.iter().map(|h| p.image(h).is_some()).collect();
    assert_eq!(DECODES_PER_FRAME, 2);
    assert_eq!(got, [true, true, false, false, false]);
    // Уже декодированные — из кэша, без бюджета.
    assert!(p.image(&hashes[0]).is_some());
    images.begin_frame();
    let p = images.provider(&store);
    let got: Vec<bool> = hashes.iter().map(|h| p.image(h).is_some()).collect();
    assert_eq!(got, [true, true, true, true, false]);
    assert_eq!(images.stats().2, 4);
    let img = p.image(&hashes[1]).unwrap();
    assert_eq!(&img.data_as_u8_slice()[..4], &[40, 0, 0, 255]);
}

#[test]
fn missing_and_broken_images_get_a_permanent_placeholder() {
    let mut store = ImageStore::default();
    let broken = store.add(b"not a png".to_vec());
    let images = DocImages::default();
    images.begin_frame();
    let p = images.provider(&store);
    // Байтов нет: заглушка сразу, без бюджета декодирования.
    let a = p.image("нет-такого").expect("заглушка");
    // Неразборчивая картинка: заглушка, и второй раз её не декодируют.
    let b = p.image(&broken).expect("заглушка");
    assert_eq!((a.width(), a.height(), b.width()), (1, 1, 1));
    images.begin_frame();
    assert!(images.provider(&store).image(&broken).is_some());
    assert_eq!(images.stats().2, 1, "неразборчивая декодировалась снова");
}

#[test]
fn memory_limit_evicts_images_not_used_this_frame() {
    let mut store = ImageStore::default();
    let a = store.add(png(16, 16, [1, 0, 0, 255]));
    let b = store.add(png(16, 16, [2, 0, 0, 255]));
    // Предел — одна картинка 16×16.
    let images = DocImages::with_limit(16 * 16 * 4);
    images.begin_frame();
    let p = images.provider(&store);
    p.image(&a).unwrap();
    // В этом же кадре `a` нужна: обе остаются, предел временно превышен.
    p.image(&b).unwrap();
    assert_eq!(images.stats().0, 2);
    images.begin_frame();
    let p = images.provider(&store);
    p.image(&b).unwrap();
    // `a` в этом кадре не рисовалась — вытесняется, когда понадобится место.
    images.insert("c", Pixmap::new(16, 16));
    assert_eq!(images.stats().0, 2);
    assert!(p.image(&b).is_some());
    let (n, bytes, decoded) = images.stats();
    assert_eq!((n, bytes, decoded), (2, 2 * 16 * 16 * 4, 2));
    // Вытеснена именно `a`: её снова приходится декодировать.
    images.begin_frame();
    images.provider(&store).image(&a).unwrap();
    assert_eq!(images.stats().2, 3);
}
