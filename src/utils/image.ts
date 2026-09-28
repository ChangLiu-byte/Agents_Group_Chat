/** Image formats every provider accepts: mime type -> canonical extension.
 * The Rust side (`attachment.rs`) re-validates everything; this is only so
 * the user gets immediate feedback instead of an error on send. */
const ALLOWED_IMAGE_TYPES: Record<string, string> = {
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/gif": "gif",
  "image/webp": "webp",
};

const EXTENSION_TO_MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
};

/** For `<input type="file" accept=...>`. */
export const IMAGE_ACCEPT = Object.keys(ALLOWED_IMAGE_TYPES).join(",");

export const UNSUPPORTED_IMAGE_HINT = "仅支持 PNG / JPEG / GIF / WebP 图片";

function extensionOf(name: string): string | null {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : null;
}

/** The file's mime type, falling back to its extension when the platform
 * didn't report one; `null` if it isn't a supported image. */
export function imageMimeType(file: File): string | null {
  if (file.type in ALLOWED_IMAGE_TYPES) return file.type;
  if (file.type !== "") return null;
  const ext = extensionOf(file.name);
  return ext ? (EXTENSION_TO_MIME[ext] ?? null) : null;
}

/** Keep in sync with `ImageUpload` in `src-tauri/src/attachment.rs`. */
export interface ImageUpload {
  file_name: string;
  mime_type: string;
  data_base64: string;
}

function readAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      // Result is "data:<mime>;base64,<data>" - only the data is sent.
      const result = reader.result as string;
      resolve(result.slice(result.indexOf(",") + 1));
    };
    reader.onerror = () => reject(reader.error ?? new Error(`读取图片 "${file.name}" 失败`));
    reader.readAsDataURL(file);
  });
}

export async function fileToImageUpload(file: File): Promise<ImageUpload> {
  const mimeType = imageMimeType(file);
  if (!mimeType) throw new Error(`不支持的图片 "${file.name}"，${UNSUPPORTED_IMAGE_HINT}`);
  // The backend needs an extension; pasted images can come without a
  // usable file name.
  const fileName = extensionOf(file.name) ? file.name : `image.${ALLOWED_IMAGE_TYPES[mimeType]}`;
  return { file_name: fileName, mime_type: mimeType, data_base64: await readAsBase64(file) };
}
