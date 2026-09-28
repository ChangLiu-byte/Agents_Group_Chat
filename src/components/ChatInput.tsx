import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type ClipboardEvent,
  type KeyboardEvent,
} from "react";
import type { Agent } from "../types/agent";
import type { SessionMode } from "../types/session";
import { IMAGE_ACCEPT, UNSUPPORTED_IMAGE_HINT, imageMimeType } from "../utils/image";

/** An image picked/pasted but not sent yet. */
interface PendingImage {
  key: string;
  file: File;
  previewUrl: string;
}

interface ChatInputProps {
  mode: SessionMode;
  running: boolean;
  roster: Agent[];
  mentionTargetId: string | null;
  onMentionTargetChange: (agentId: string) => void;
  onModeChange: (mode: SessionMode) => void;
  onSend: (text: string, images: File[]) => void;
}

const modeButtonClass = (active: boolean) =>
  "rounded-md px-3 py-1.5 text-sm font-medium transition-colors " +
  (active ? "bg-indigo-600 text-white" : "border border-slate-300 text-slate-600 hover:bg-slate-100");

/**
 * Message box with the two mode buttons inside it. "小组发言" (sequential)
 * is the default; "点名发言" (mention) expands the session's agents so one
 * can be picked, and clicking "小组发言" again switches back. The picked
 * agent (and mode) are owned by the parent, so they survive re-renders and
 * carry over to the next round unless the user changes them.
 *
 * Images can be attached with the "图片" button or pasted into the text box;
 * several per message are allowed, and each can be removed before sending.
 */
export function ChatInput({
  mode,
  running,
  roster,
  mentionTargetId,
  onMentionTargetChange,
  onModeChange,
  onSend,
}: ChatInputProps) {
  const [text, setText] = useState("");
  const [images, setImages] = useState<PendingImage[]>([]);
  const [imageError, setImageError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Free the preview object URLs of whatever is still pending on unmount.
  const imagesRef = useRef(images);
  imagesRef.current = images;
  useEffect(() => () => imagesRef.current.forEach((i) => URL.revokeObjectURL(i.previewUrl)), []);

  const mentionMode = mode === "mention";
  const mentionTargetValid = mentionTargetId !== null && roster.some((a) => a.id === mentionTargetId);
  const blockedReason = running
    ? "本轮进行中，请等待所有 Agent 发言完毕"
    : roster.length === 0
      ? "名单里还没有 Agent，先在上方“管理名单”里添加"
      : mentionMode && !mentionTargetValid
        ? "请先选择点名对象"
        : null;
  const canSend = blockedReason === null && (text.trim() !== "" || images.length > 0);

  // Who will receive these images but can't see them: the whole roster in
  // sequential mode, only the picked agent in mention mode.
  const recipients = mentionMode ? roster.filter((a) => a.id === mentionTargetId) : roster;
  const blindRecipients = images.length > 0 ? recipients.filter((a) => !a.supports_vision) : [];

  function addImages(files: File[]) {
    const accepted = files.filter((f) => imageMimeType(f) !== null);
    const rejected = files.filter((f) => imageMimeType(f) === null);
    setImageError(
      rejected.length > 0
        ? `已忽略 ${rejected.map((f) => f.name || "粘贴的文件").join("、")}：${UNSUPPORTED_IMAGE_HINT}`
        : null,
    );
    if (accepted.length === 0) return;
    setImages((prev) => [
      ...prev,
      ...accepted.map((file) => ({
        key: crypto.randomUUID(),
        file,
        previewUrl: URL.createObjectURL(file),
      })),
    ]);
  }

  function removeImage(key: string) {
    setImages((prev) => {
      const removed = prev.find((i) => i.key === key);
      if (removed) URL.revokeObjectURL(removed.previewUrl);
      return prev.filter((i) => i.key !== key);
    });
  }

  function handleFileChange(e: ChangeEvent<HTMLInputElement>) {
    addImages(Array.from(e.target.files ?? []));
    // Reset so picking the same file again still fires onChange.
    e.target.value = "";
  }

  function handlePaste(e: ClipboardEvent<HTMLTextAreaElement>) {
    const files = Array.from(e.clipboardData.files);
    if (files.length === 0) return; // plain text paste - leave it alone
    e.preventDefault();
    addImages(files);
  }

  function send() {
    if (!canSend) return;
    onSend(
      text.trim(),
      images.map((i) => i.file),
    );
    images.forEach((i) => URL.revokeObjectURL(i.previewUrl));
    setImages([]);
    setImageError(null);
    setText("");
  }

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    // Enter while a Chinese/Japanese IME is composing only confirms the
    // candidate, it must not send the message.
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      send();
    }
  }

  return (
    <div className="rounded-lg border border-slate-300 bg-white p-3 shadow-sm focus-within:border-indigo-500 space-y-2">
      {mentionMode && roster.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-slate-500">点名对象</span>
          {roster.map((agent) => (
            <button
              key={agent.id}
              type="button"
              onClick={() => onMentionTargetChange(agent.id)}
              className={
                "flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs " +
                (mentionTargetId === agent.id
                  ? "border-indigo-500 bg-indigo-50 text-indigo-700"
                  : "border-slate-300 text-slate-600 hover:bg-slate-50")
              }
            >
              <span
                className="h-2 w-2 rounded-full"
                style={{ backgroundColor: agent.color ?? "#94a3b8" }}
                aria-hidden
              />
              {agent.name}
            </button>
          ))}
        </div>
      )}

      {images.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {images.map((image) => (
            <div key={image.key} className="relative h-16 w-16">
              <img
                src={image.previewUrl}
                alt={image.file.name}
                className="h-16 w-16 rounded-md border border-slate-200 object-cover"
              />
              <button
                type="button"
                onClick={() => removeImage(image.key)}
                title="移除这张图片"
                className="absolute -right-1.5 -top-1.5 flex h-5 w-5 items-center justify-center rounded-full bg-slate-700 text-xs leading-none text-white hover:bg-slate-900"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      )}

      {imageError && <p className="text-xs text-red-600">{imageError}</p>}
      {blindRecipients.length > 0 && (
        <p className="text-xs text-amber-600">
          {blindRecipients.map((a) => a.name).join("、")} 未开启图片识别，只会收到“附带了图片”的文字提示
        </p>
      )}

      <textarea
        rows={3}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
        placeholder="输入消息，Enter 发送，Shift+Enter 换行，可直接粘贴图片"
        className="w-full resize-none border-0 p-0 text-sm text-slate-900 focus:outline-none focus:ring-0"
      />

      <div className="flex items-center justify-between gap-3">
        <p className="text-xs text-slate-400">{blockedReason}</p>
        <div className="flex shrink-0 items-center gap-2">
          <input
            ref={fileInputRef}
            type="file"
            accept={IMAGE_ACCEPT}
            multiple
            className="hidden"
            onChange={handleFileChange}
          />
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            title="添加图片（PNG / JPEG / GIF / WebP）"
            className="rounded-md border border-slate-300 px-3 py-1.5 text-sm text-slate-600 hover:bg-slate-100"
          >
            📎 图片
          </button>
          <button
            type="button"
            onClick={() => onModeChange("sequential")}
            className={modeButtonClass(!mentionMode)}
          >
            小组发言
          </button>
          <button
            type="button"
            onClick={() => onModeChange("mention")}
            className={modeButtonClass(mentionMode)}
          >
            点名发言
          </button>
          <button
            type="button"
            onClick={send}
            disabled={!canSend}
            className="rounded-md bg-indigo-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-40"
          >
            {running ? "发言中..." : "发送"}
          </button>
        </div>
      </div>
    </div>
  );
}
