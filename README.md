# Unity L10n Tool (Rust + Tauri)

Công cụ việt hóa game Unity đóng gói kiểu XAPK/APK + YooAsset, với text nằm trong bảng FlatBuffers.
Viết hoàn toàn bằng Rust, giao diện Tauri. Không cần Python. Chỉ bước ký APK cần Java.

Giao diện có **trình soạn thảo bảng ngay trong ứng dụng**: nhấp đúp (hoặc chuột phải) vào một bảng ở
kết quả quét để mở nó trong một tab và sửa trực tiếp từng ô, không cần qua Excel. Có **4 giao diện**
(Tự động / Sáng / Tối / Tối + Nâu), **lưu cấu hình và lịch sử** vào Documents, và **phiên bản để rollback**.

## Chạy

- `UnityL10nTool.exe` — ứng dụng chính. Cần WebView2 (Windows 11 có sẵn).
- `libra.pak` — chứa công cụ ký (uber-apk-signer) đã **nén và mã hóa**. Để cạnh file `.exe`.
- `l10n-cli.exe` — bản dòng lệnh dùng chung lõi (tuỳ chọn; gõ không tham số để xem cách dùng).

## Quy trình

1. **Giải nén** — chọn file XAPK và thư mục làm việc, tách thành các APK con và giải nén.
2. **Quét** — nhận diện engine, liệt kê các bảng config (bảng nhiều chữ xếp trước). Bảng đang có bản sửa
   mang nhãn **đã sửa**.
3. **Sửa text** — hai cách, dùng cách nào cũng được, kết quả ghi về cùng chỗ (`work\mod`):
   - **Trong ứng dụng:** nhấp đúp vào một bảng để mở tab, sửa từng ô, nhấn **Lưu** (Ctrl+S).
   - **Qua Excel:** bước **Xuất ra Excel** → sửa file `.xlsx` → **Nhập vào bundle**.
4. **Đóng gói & ký** — ra `work\out\signed\*.apk`. Cài bằng `adb install-multiple` (gỡ bản gốc trước
   vì khác chữ ký).

## Trình soạn thảo bảng (tab)

- **Lưới ảo hóa**: mở bảng 23.000+ dòng vẫn mượt, dữ liệu lấy theo khối từ Rust.
- **Sửa ô**: nhấp đúp hoặc nhấn **Enter/F2**. Enter lưu ô và xuống dòng dưới, Esc huỷ, Tab sang cột.
- **Chuột phải** vào ô: sửa, sao chép, sao chép/khôi phục giá trị gốc.
- **Chuột phải** vào tiêu đề bảng ở bước Quét: mở để sửa, xuất riêng ra Excel, hoặc **khôi phục bản gốc**.
- **Lọc**: ô tìm kiếm, chọn cột, và lọc nhanh "đã sửa so với gốc / chưa lưu / có ô trống".
- **Cột**: đổi tên (nhấp đúp tiêu đề), kéo giãn, ẩn/hiện qua nút **Cột ▾**. Thiết lập được nhớ cho lần sau.
- **Cột không phải chuỗi** (chữ đơn cách, tiêu đề ghi kiểu `int`, `float`, `int[]`, `str[]`, `obj[]`…) cũng
  sửa được. Kiểu được suy ra từ dữ liệu vì không có schema:
  - số: gõ số mới; mảng: `[1, 4]`, `[8.4, -7.0]`, `["a", "b"]` — thêm/bớt phần tử tuỳ ý;
  - mảng bảng con: `[{"f0": [10101001, 10]}, …]` — mỗi phần tử phải có đúng bộ trường như một phần tử có
    sẵn trong cột (rê chuột vào nhãn kiểu ở tiêu đề để xem các trường);
  - giá trị sai kiểu/cú pháp bị từ chối, ô giữ nguyên. Cột ghi `?` hoặc `rỗng` là chỉ đọc vì không xác định
    chắc được kiểu.
  - Các cột này không nằm trong file Excel (Excel chỉ chứa cột chuỗi).
- Ô **đã sửa** có viền cam; ô **chưa lưu** in nghiêng kèm chấm ●. Ô bị gạch chéo là trường vắng mặt
  trong dữ liệu gốc, không sửa được.
- **Lưu** chỉ ghi các ô khác bản gốc, luôn dựng lại bundle từ bản gốc nên lưu nhiều lần không phình file,
  và tự đọc lại bundle để kiểm tra trước khi thay thế.

## Tab "Tệp game" — duyệt, xem và trích xuất tài nguyên

Bố cục giống Windows Explorer (khi mở tab này, cột các bước bên trái tự ẩn; nút ☰ để hiện lại):

- **Thanh điều hướng**: ← → ↑ ⟳, thanh địa chỉ dạng breadcrumb (nhấp từng cấp để nhảy tới; nhấp chỗ trống
  hoặc Ctrl+L để gõ đường dẫn), ô tìm kiếm tệp theo tên trong thư mục làm việc (Ctrl+E).
- Bốn khung từ trái sang phải: **cây thư mục** (bật/tắt: 🌲) · **(2) tệp và thư mục** · **(3) nội dung bundle**
  · **(4) xem nội dung** (bật/tắt: ▣ hoặc Alt+P).
  - Khung 2 chỉ liệt kê tên tệp (đoán loại theo đuôi, không mở tệp) nên thư mục 1.700+ bundle hiện ngay.
  - **Chọn** một 📦 bundle hoặc 🎵 FMOD bank ở khung 2 → lúc đó mới đọc tệp đó và liệt kê các tệp con ở khung 3,
    không cần giải nén. Tệp đuôi lạ (vd. `.rawfile`) chỉ được nhận dạng khi bạn chọn nó.
    Khung 3 tự chọn tệp con đầu tiên và hiện nội dung ở khung 4 ngay; con trỏ vẫn ở khung 2 nên bấm ↑/↓
    để lướt nhanh qua từng bundle.
  - **Chọn** một tệp con ở khung 3 (hoặc tệp rời ở khung 2) → lúc đó mới giải mã và hiện ở khung 4:
    văn bản, ảnh, âm thanh (OGG/MP3/WAV/M4A/AAC/FLAC/Opus, FMOD bank), video.
  - FMOD bank trong bundle hiện dạng cây: ▾ để thu gọn/mở rộng các âm thanh bên trong.
  - Kéo đường chia giữa các khung để đổi cỡ (nhấp đúp đường chia để về mặc định). Mỗi khung hẹp nhất vẫn thấy
    ~20 ký tự tên tệp; các cột còn lại thì cuộn ngang.
- **Nhật ký** bên dưới: nhấp thanh tiêu đề hoặc nút −/+ để thu gọn/mở rộng; khi thu gọn sẽ báo số dòng mới.
- **Chế độ xem**: *Chi tiết* (cột Tên / Loại / Kích thước / Ngày sửa / Thông tin — nhấp tiêu đề để sắp xếp,
  kéo mép để đổi độ rộng), *Biểu tượng lớn*, *Biểu tượng vừa* (ảnh/sprite hiện ảnh thu nhỏ), *Danh sách*.
- **Chọn nhiều** bằng Ctrl/Shift+nhấp, Ctrl+A; thanh trạng thái hiện số mục và tổng dung lượng đã chọn.
- Phím tắt: Enter mở, Backspace / Alt+← quay lại, Alt+→ tiến tới, Alt+↑ lên một cấp, F5 làm mới,
  Ctrl+C sao chép đường dẫn; nút Back/Forward của chuột cũng dùng được.
- **Bundle Unity** (📦, gồm cả `.bundle` và `.rawfile` của YooAsset) và **FMOD bank** (🎵) mở ra như thư mục: bên trong liệt kê
  ảnh (Texture2D, Sprite), âm thanh (từng bài trong FMOD bank, AudioClip), video (VideoClip), text
  (TextAsset, bảng FlatBuffers), font. Bật **Hiện mọi đối tượng** để thấy cả GameObject, Material…
- **Xem trước** ở khung phải: đường dẫn đầy đủ, thông tin (kích thước, định dạng nén, thời lượng…), và
  - ảnh: giải mã ASTC / ETC / ETC2 / DXT / BC / PVRTC / Crunch… ra PNG, nhấp để xem cỡ thật;
  - âm thanh: FMOD bank (FSB5 Vorbis) dựng lại thành **OGG** phát ngay, không nén lại nên không giảm chất lượng;
  - video: phát MP4, tua được;
  - text: hiện nội dung (bảng FlatBuffers hiện dạng TSV); tệp nhị phân hiện dạng hex.
- **Trích xuất một mục**: nút *Trích xuất…* / chuột phải → lưu ảnh `.png`, âm thanh `.ogg`, video `.mp4`, text `.tsv/.txt/.json`.
- **Trích xuất hàng loạt**: chọn loại (text, ảnh, sprite, âm thanh, video, font, dữ liệu khác) và thư mục đích;
  nguồn là thư mục/bundle đang chọn hoặc toàn bộ game. Chạy song song nhiều luồng
  (toàn bộ game này: ~9.500 tệp, 2,6 GB trong khoảng 15 giây). Hai cách xếp tệp (chọn ở góc trái hộp thoại):
  - **Xuất tách theo thư mục gốc** (mặc định): mỗi bundle ra một thư mục con, tệp media rời trong `_files`.
  - **Xuất chung 1 thư mục**: mọi tệp vào thẳng thư mục đích; trùng tên giữa các bundle thì thêm `_2`, `_3`…
- Một số mục báo lỗi là bình thường: texture font động (không kèm dữ liệu ảnh), sprite nằm trong atlas ở bundle khác.

## Giao diện (theme)

Chọn ở góc phải trên: **Tự động** (theo hệ điều hành), **Sáng**, **Tối**, **Tối + Nâu**. Lựa chọn được
lưu vào cấu hình nên giữ nguyên cho lần mở sau. Mọi màu dùng biến CSS nên textbox, nút, lưới và hộp thoại
đều tương phản đúng ở cả bốn giao diện.

## Cấu hình, lịch sử và rollback

Lưu trong thư mục `Documents\UnityL10nTool\` của tài khoản đang đăng nhập:

| File/thư mục | Nội dung |
|---|---|
| `config.json` | Giao diện, thời hạn giữ log, đường dẫn gần đây |
| `history.jsonl` | Lịch sử: mỗi dòng một sự kiện (giải nén, quét, xuất, nhập, mở bảng, lưu, ký, phục hồi) kèm mốc thời gian |
| `backups\<id>\` | Các phiên bản của từng bảng đã sửa, để phục hồi |

- Nút **Lịch sử** (góc phải trên) mở danh sách việc đã làm, lọc theo thư mục làm việc.
- Mỗi lần **Lưu** một bảng, một **phiên bản** được lưu lại (bỏ qua nếu trùng bản mới nhất). Chuột phải vào
  bảng (ở kết quả quét hoặc trên tab) → **Phiên bản / phục hồi…** để xem danh sách và **phục hồi** về bất kỳ
  phiên bản nào. Phục hồi xong, tab đang mở sẽ tự tải lại.
- Nút **⚙ Cấu hình**: chọn giao diện, đặt **thời hạn giữ lịch sử/bản sao lưu** (ngày; 0 = giữ mãi mãi, luôn
  giữ ít nhất bản mới nhất của mỗi bảng), mở thư mục dữ liệu, và **dọn log/temp cũ ngay**. Khi mở ứng dụng,
  bản sao lưu và lịch sử cũ hơn thời hạn sẽ tự được dọn.

## libra.pak — gói phụ thuộc đã mã hóa

Các file phụ thuộc lúc chạy (hiện là `uber-apk-signer.jar`) được gộp vào một file `libra.pak`:

- Nén LZ4 rồi mã hóa **AES-256-GCM**; khóa suy ra từ mật khẩu `cuongpzo` bằng **PBKDF2-HMAC-SHA256**
  (200.000 vòng, muối ngẫu nhiên lưu trong file).
- Mở bằng trình xem hex chỉ thấy byte ngẫu nhiên (ngoài 9 byte chữ ký `LIBRAPAK`); không chương trình nào
  khác giải nén hay đọc được nếu không có mật khẩu. GCM có xác thực nên sửa 1 byte là giải mã thất bại,
  không trả về dữ liệu rác.
- Chỉ `UnityL10nTool.exe`/`l10n-cli.exe` (có sẵn mật khẩu) đọc được. Khi ký, chương trình giải mã jar ra
  một file tạm, chạy xong thì **xóa ngay**.

**Giới hạn thực tế (nói thẳng):** mật khẩu `cuongpzo` phải nằm trong file `.exe` để chương trình tự giải
mã được, nên người có kỹ năng dịch ngược `.exe` vẫn moi ra được. Đây là lớp bảo vệ chống đọc/sửa trực tiếp
file `.pak`, **không phải** mã hóa bất khả xâm phạm. Muốn mạnh hơn thì mật khẩu phải do người dùng nhập lúc
chạy, không nhúng vào chương trình.

Tạo/giải nén thủ công bằng CLI:

```
l10n-cli mkpak libra.pak uber-apk-signer-1.3.0.jar
l10n-cli unpak libra.pak thu_muc_ra
```

## Build

```
powershell -ExecutionPolicy Bypass -File build-release.ps1
```

Script tự tắt ứng dụng đang chạy, build cả workspace, tạo `libra.pak` từ jar, rồi xuất `UnityL10nTool.exe`
+ `libra.pak` + `README.md` ra thư mục `..\Release` (ngoài dự án). Thêm `-Zip` để nén, `-WithCli` để kèm
`l10n-cli.exe`. Cần Rust (MSVC) và Visual Studio Build Tools; giao diện là HTML/CSS/JS thuần, không cần Node.

## Cấu trúc

| Đường dẫn | Nội dung |
|---|---|
| `core\src\unityfs.rs` | Đọc/ghi bundle UnityFS (LZ4/LZMA), giữ byte chèn trước chữ ký của YooAsset |
| `core\src\serialized.rs` | Đọc bảng đối tượng SerializedFile (bản 17+), đọc/ghi TextAsset |
| `core\src\flat.rs` | Bảng FlatBuffers không cần schema; sửa ô bằng cách nối chuỗi mới vào cuối buffer |
| `core\src\pak.rs` | Gói/mã hóa libra.pak (AES-256-GCM + PBKDF2) |
| `core\src\lib.rs` | Các bước unpack / scan / export / import / pack và đọc/ghi bảng cho trình soạn thảo |
| `assets\src\sfile.rs` | Đọc đối tượng bất kỳ theo type tree trong SerializedFile |
| `assets\src\tex.rs` | Giải mã Texture2D (ASTC, ETC2, DXT, BC7, Crunch…) → PNG, cắt sprite |
| `assets\src\fsb.rs` | FMOD FSB5: liệt kê bài, Vorbis → Ogg, PCM → WAV |
| `assets\src\lib.rs` | Liệt kê/xuất tài nguyên trong bundle, trích xuất hàng loạt song song |
| `assets\examples\dump.rs` | Công cụ thử dòng lệnh: `cargo run -r -p l10n-assets --example dump -- list <bundle>` |
| `src-tauri\src\files.rs` | Lệnh cho tab Tệp game và giao thức `gfs://` phát ảnh/âm thanh/video (hỗ trợ tua) |
| `src-tauri\src\main.rs` | Các lệnh Tauri |
| `src-tauri\src\tables.rs` | Phiên soạn thảo bảng trong bộ nhớ (mở/lọc/sửa/lưu) |
| `src-tauri\src\store.rs` | Cấu hình, lịch sử, bản sao lưu/rollback trong Documents, dọn theo thời hạn |
| `ui\` | Giao diện: `main.js` (các bước + theme + cấu hình + lịch sử), `editor.js` (tab + lưới + menu + phiên bản), `files.js` (tab Tệp game) |

## Giới hạn

- Chỉ hỗ trợ SerializedFile little-endian bản 17+ (Unity 2019+) và bundle không mã hóa ngoài kiểu chèn byte.
- Bundle ghi ra nén LZ4 theo khối 128 KB (có thể lớn hơn bản gốc nếu bản gốc nén LZMA).
- Chưa thử APK đã sửa trên điện thoại thật.
- Nhật ký trong ứng dụng viết tiếng Việt không dấu (từ phần lõi).
