import { useEffect } from "react";

export interface ToastMsg {
  id: number;
  message: string;
  kind: "info" | "success" | "error";
}

export function Toast({ toast, onDone }: { toast: ToastMsg; onDone: () => void }) {
  useEffect(() => {
    const t = setTimeout(onDone, toast.kind === "error" ? 6000 : 3200);
    return () => clearTimeout(t);
  }, [toast.id, toast.kind, onDone]);

  return (
    <div className={`toast toast-${toast.kind}`} onClick={onDone}>
      {toast.message}
    </div>
  );
}
