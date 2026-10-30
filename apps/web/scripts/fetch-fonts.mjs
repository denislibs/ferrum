// Вшитые шрифты (спека подсистемы 3, раздел 4): скачивает файлы из репозитория
// google/fonts на закреплённом коммите, сверяет SHA-256 и кладёт их с лицензиями
// в public/fonts/<каталог>/. Файлы лежат в git: скрипт нужен, чтобы обновить их
// или проверить, что они те же (`--check`). Манифест (`manifest.json`) строит
// из файлов `cargo run -p ferrum-text --example font_manifest`.
//
// Все шрифты — SIL Open Font License 1.1 (OFL.txt рядом с файлами семейства).
// Onest в public/fonts нет: он уже вшит в wasm (crates/ferrum-text/assets) и
// регистрируется ядром без загрузки.

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Коммит google/fonts от 2026-10-09. */
export const COMMIT = 'bd8f81ddb5c74d5c8897b36ad88b440266245103';
const BASE = `https://raw.githubusercontent.com/google/fonts/${COMMIT}/ofl/`;
const OUT = fileURLToPath(new URL('../public/fonts/', import.meta.url));

/** [путь в google/fonts, файл в public/fonts, SHA-256]. */
export const FILES = [
  ['inter/Inter[opsz,wght].ttf', 'inter/Inter-Variable.ttf', '29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031'],
  ['inter/Inter-Italic[opsz,wght].ttf', 'inter/Inter-Italic-Variable.ttf', 'acd98e64795781b2058f07b18475e0ecee2a0fe2b42a49e2f9e37d0d6bf66ce6'],
  ['inter/OFL.txt', 'inter/OFL.txt', '5b9321a4298cfeb6b34354164a1c3afc3db114569984c502b9b35d988fd58c57'],
  ['roboto/Roboto[wdth,wght].ttf', 'roboto/Roboto-Variable.ttf', 'd7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134'],
  ['roboto/Roboto-Italic[wdth,wght].ttf', 'roboto/Roboto-Italic-Variable.ttf', '9725a847af6b460ffca162ae66d20dad48b01876137947180b42d7dcd7887182'],
  ['roboto/OFL.txt', 'roboto/OFL.txt', '061402327a96aadb0bfb694a960ed289ecd38d383e396243831ab81feb109c41'],
  ['opensans/OpenSans[wdth,wght].ttf', 'opensans/OpenSans-Variable.ttf', '36643644f318a812aab2d2ed3bb98f8cf0872527f835fe9398d95fe6b9adb878'],
  ['opensans/OpenSans-Italic[wdth,wght].ttf', 'opensans/OpenSans-Italic-Variable.ttf', 'fe269381e992f32e135801740998544d6235061e37c93ec067ad2be3edd5b17b'],
  ['opensans/OFL.txt', 'opensans/OFL.txt', 'fbbbcfef55318de350562559b671360de6d597112ecc5c73881b05092db89602'],
  ['montserrat/Montserrat[wght].ttf', 'montserrat/Montserrat-Variable.ttf', '0f7b311b2f3279e4eef9b2f968bcdbab6e28f4daeb1f049f4f278a902bcd82f7'],
  ['montserrat/Montserrat-Italic[wght].ttf', 'montserrat/Montserrat-Italic-Variable.ttf', '51607f316bc020e59f03cbf51543eecffbea501c0b31d73e5b82927c5cca442c'],
  ['montserrat/OFL.txt', 'montserrat/OFL.txt', '8b7141c03fa4f8d44e6345d5d4931709290f0f67875e452e95ac1fd3a027802e'],
  ['ptsans/PT_Sans-Web-Regular.ttf', 'ptsans/PTSans-Regular.ttf', '9cc831490532009bae2b3ce0d39c62adfc889060beb421593bfd9d2396d0f10a'],
  ['ptsans/PT_Sans-Web-Bold.ttf', 'ptsans/PTSans-Bold.ttf', '3128bd5ecf01816e59a23d54c57a7a6b14615b07db53ff277c77376010265b05'],
  ['ptsans/PT_Sans-Web-Italic.ttf', 'ptsans/PTSans-Italic.ttf', '5a90fe2d0cd798700935240580bdcc12c0ffc9102c0c7163b3418e13bc21debd'],
  ['ptsans/PT_Sans-Web-BoldItalic.ttf', 'ptsans/PTSans-BoldItalic.ttf', '81ac221cdd02bccfa679c74adb122478e9d092e65a722e31ca11469961483785'],
  ['ptsans/OFL.txt', 'ptsans/OFL.txt', '2758cf7a872827f39661cf8cc24188113c030447aefb5ca7145993650076ca8c'],
  ['ptserif/PT_Serif-Web-Regular.ttf', 'ptserif/PTSerif-Regular.ttf', 'a4951fade06ff8f09b7673aa81ffb65a8cd409e24d3289a6dc670bc4dda2557a'],
  ['ptserif/PT_Serif-Web-Bold.ttf', 'ptserif/PTSerif-Bold.ttf', '038ba7336bd7ea14f12ad155bed51a4345cac5153275d521dec3ba04021c526e'],
  ['ptserif/PT_Serif-Web-Italic.ttf', 'ptserif/PTSerif-Italic.ttf', 'f57e95ff9dc85691a3b2e193f2028db36f6663939a46c0fc4f286d618b80b7ce'],
  ['ptserif/PT_Serif-Web-BoldItalic.ttf', 'ptserif/PTSerif-BoldItalic.ttf', 'f003788ba08981eb0988b3557a6f224a53dab49c20e283e8b74d5af3c466f8be'],
  ['ptserif/OFL.txt', 'ptserif/OFL.txt', 'ddf311c28ddf5a5ad9747649837346b67bed9d356789c3072bb27dbce49e514d'],
  ['notosans/NotoSans[wdth,wght].ttf', 'notosans/NotoSans-Variable.ttf', 'bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d'],
  ['notosans/NotoSans-Italic[wdth,wght].ttf', 'notosans/NotoSans-Italic-Variable.ttf', '58e6e0ebd1931b29a365aa2d3e2ee9a9e831a3af7cf3ad1462d4e72154f0b291'],
  ['notosans/OFL.txt', 'notosans/OFL.txt', 'cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a'],
  ['jetbrainsmono/JetBrainsMono[wght].ttf', 'jetbrainsmono/JetBrainsMono-Variable.ttf', '48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda'],
  ['jetbrainsmono/JetBrainsMono-Italic[wght].ttf', 'jetbrainsmono/JetBrainsMono-Italic-Variable.ttf', '85ae2a5cd3f56baf1ce1c21a851322c58e3d8fbe8e8ad4a4d090a820dd7fe558'],
  ['jetbrainsmono/OFL.txt', 'jetbrainsmono/OFL.txt', 'b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d'],
  ['playfairdisplay/PlayfairDisplay[wght].ttf', 'playfairdisplay/PlayfairDisplay-Variable.ttf', 'c40f2293766a503bc70cce9e512ef844a4ccb7cbcde792fe2ea31d191917d8d6'],
  ['playfairdisplay/PlayfairDisplay-Italic[wght].ttf', 'playfairdisplay/PlayfairDisplay-Italic-Variable.ttf', 'a5e26dc5e2e77fb2803a0bf02fd4f81ee136ec8dea863ccdb0c59a263b21378b'],
  ['playfairdisplay/OFL.txt', 'playfairdisplay/OFL.txt', '566be814f8e96e93dfa16101331557eb6b5467e9e03f627c0910fe93ca12300e'],
];

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const check = process.argv.includes('--check');
let failed = 0;
for (const [src, dst, hash] of FILES) {
  const path = OUT + dst;
  if (existsSync(path) && sha256(readFileSync(path)) === hash) continue;
  if (check) {
    console.error(`public/fonts/${dst}: нет или не совпадает SHA-256`);
    failed++;
    continue;
  }
  // Скобки и запятые в именах файлов google/fonts — в адресе экранируются.
  const url = BASE + src.split('/').map(encodeURIComponent).join('/');
  const res = await fetch(url);
  if (!res.ok) {
    console.error(`${url}: HTTP ${res.status}`);
    failed++;
    continue;
  }
  const bytes = Buffer.from(await res.arrayBuffer());
  if (sha256(bytes) !== hash) {
    console.error(`${url}: SHA-256 не совпал`);
    failed++;
    continue;
  }
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes);
  console.log(`public/fonts/${dst} (${bytes.length} байт)`);
}
if (failed) process.exit(1);
