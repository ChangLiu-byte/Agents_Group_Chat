import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { appDataDir, join } from "@tauri-apps/api/path";
import type { Attachment } from "../types/session";

// Resolved once and shared by every image.
let attachmentsDirPromise: Promise<string> | null = null;
function attachmentsDir(): Promise<string> {
  attachmentsDirPromise ??= appDataDir().then((dir) => join(dir, "attachments"));
  return attachmentsDirPromise;
}

interface AttachmentImageProps {
  attachment: Attachment;
  className?: string;
  onClick?: () => void;
}

/** A stored attachment, loaded straight from disk through Tauri's asset
 * protocol (scope: `$APPDATA/attachments/**` in `tauri.conf.json`) - the
 * image bytes never go through IPC. */
export function AttachmentImage({ attachment, className, onClick }: AttachmentImageProps) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setFailed(false);
    attachmentsDir()
      .then((dir) => join(dir, attachment.file_path))
      .then((path) => {
        if (!cancelled) setSrc(convertFileSrc(path));
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [attachment.file_path]);

  if (failed) {
    return (
      <div
        className={
          "flex items-center justify-center bg-slate-200 text-xs text-slate-500 " + (className ?? "")
        }
      >
        图片已丢失
      </div>
    );
  }
  if (!src) {
    return <div className={"animate-pulse bg-slate-200 " + (className ?? "")} />;
  }
  return (
    <img
      src={src}
      alt="附件图片"
      className={className}
      onClick={onClick}
      onError={() => setFailed(true)}
    />
  );
}
