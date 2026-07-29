// Shared file-dialog filters and layout constants.

export const ASSET_FILTER = {
  name: "AE scripts & plugins",
  extensions: ["jsx", "js", "jsxbin", "plugin", "aex", "zxp", "pkg", "exe", "msi"],
};
export const IMAGE_FILTER = { name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] };

// Pluxel Package File — our export format. It is a plain zip under the hood, so
// legacy `.zip` files still import; the save dialog produces `.ppf`.
export const PPF_EXT = "ppf";
export const PPF_FILTER = { name: "Pluxel Package", extensions: [PPF_EXT] };
export const PPF_IMPORT_FILTER = { name: "Pluxel Package", extensions: [PPF_EXT, "zip"] };
