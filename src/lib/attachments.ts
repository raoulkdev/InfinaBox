// Files attached to a chat message: saved in the project by `chat_attach`,
// then named at the end of the message so the AI opens them and the
// transcript can show them.

const MARKER = "[Attached files — open them with your Read tool]";
const IMAGE = /\.(png|jpe?g|gif|webp|bmp)$/i;

/** The message as sent: the text, then the attached files' paths. */
export function withAttachments(text: string, paths: string[]): string {
  if (paths.length === 0) return text;
  const body = text.trim() || "Please have a look at what I attached.";
  return `${body}\n\n${MARKER}\n${paths.map((p) => `- ${p}`).join("\n")}`;
}

/** The inverse of `withAttachments`, for showing a sent message. */
export function splitAttachments(text: string): { text: string; paths: string[] } {
  const at = text.lastIndexOf(`\n\n${MARKER}\n`);
  if (at < 0) return { text, paths: [] };
  const paths = text
    .slice(at + MARKER.length + 3)
    .split("\n")
    .map((l) => l.replace(/^- /, "").trim())
    .filter(Boolean);
  return { text: text.slice(0, at), paths };
}

export const isImagePath = (path: string) => IMAGE.test(path);

/** A saved attachment's name as shown: without the timestamp prefix. */
export function attachmentLabel(path: string): string {
  const file = path.split("/").pop() ?? path;
  return file.replace(/^\d+(-\d+)?-/, "");
}

export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("That file couldn't be read."));
    reader.onload = () => {
      const url = String(reader.result);
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.readAsDataURL(file);
  });
}

/** Pasted or dropped pictures often have no useful name. */
export function nameFor(file: File): string {
  if (file.name && file.name !== "image.png") return file.name;
  const ext = file.type.split("/")[1]?.replace("jpeg", "jpg") || "png";
  return `picture.${ext}`;
}
