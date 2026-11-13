import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ChevronLeft, ChevronRight, ImageOff, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { qk, invalidateMoney } from "@/lib/queryKeys";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { toast } from "@/stores/toast";

interface AttachmentRow {
  id: string;
  transactionId: string;
  sha256: string;
  fileName: string;
  mime?: string;
}

function useAttachments(txId: string) {
  const atts = useQuery({
    queryKey: qk.entity("attachments"),
    queryFn: () => api.entityList<AttachmentRow>("attachments"),
  });
  return (atts.data ?? []).filter((a) => a.transactionId === txId);
}

/** One thumbnail; the path query lives here so lists can't hit hooks-in-loop. */
function AttachmentThumb({
  att,
  className,
  onClick,
}: {
  att: AttachmentRow;
  className?: string;
  onClick?: () => void;
}) {
  const { t } = useTranslation();
  const path = useQuery({
    queryKey: qk.attachmentPath(att.sha256),
    queryFn: () => api.attachmentPath(att.sha256),
    retry: false,
  });
  if (path.isError) {
    return (
      <span
        title={t("ui.photoPending")}
        className={cn(
          "grid place-items-center rounded-lg bg-muted text-muted-foreground ring-1 ring-border",
          className,
        )}
      >
        <ImageOff className="h-4 w-4" />
      </span>
    );
  }
  if (!path.data) {
    return <span className={cn("rounded-lg bg-muted ring-1 ring-border", className)} />;
  }
  return (
    <img
      src={convertFileSrc(path.data)}
      alt={att.fileName}
      onClick={onClick}
      className={cn("rounded-lg object-cover ring-1 ring-border", onClick && "cursor-pointer", className)}
    />
  );
}

/** Full-size photo inside the viewer dialog (same cached path query). */
function AttachmentView({ att }: { att: AttachmentRow }) {
  const { t } = useTranslation();
  const path = useQuery({
    queryKey: qk.attachmentPath(att.sha256),
    queryFn: () => api.attachmentPath(att.sha256),
    retry: false,
  });
  if (!path.data) {
    return <p className="text-sm text-muted-foreground">{t("ui.photoPending")}</p>;
  }
  return (
    <img
      src={convertFileSrc(path.data)}
      alt={att.fileName}
      className="max-h-[65vh] w-auto rounded-lg object-contain"
    />
  );
}

/** Thumbnail strip of one transaction's photos: click → full view, delete → undo toast. */
export function TxAttachments({ txId, editable = false }: { txId: string; editable?: boolean }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [view, setView] = useState<number | null>(null);
  const mine = useAttachments(txId);

  const del = useMutation({
    mutationFn: (id: string) => api.entityDelete("attachments", id),
    onSuccess: () => invalidateMoney(qc),
  });
  const remove = (att: AttachmentRow) => {
    del.mutate(att.id);
    toast(t("ui.photoDeleted"), {
      actionLabel: t("ui.undo"),
      onAction: () => {
        void api.entityRestore("attachments", att.id).then(() => invalidateMoney(qc));
      },
    });
  };

  if (mine.length === 0) return null;
  const shown = view !== null ? mine[view] : undefined;

  return (
    <div className="flex flex-wrap gap-2">
      {mine.map((att, i) => (
        <span key={att.id} className="relative">
          <AttachmentThumb att={att} className="h-14 w-14" onClick={() => setView(i)} />
          {editable && (
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              title={t("ui.delete")}
              className="absolute -right-1 -top-1 h-5 w-5 rounded-full bg-card/90 text-muted-foreground shadow-sm hover:text-destructive"
              onClick={(e) => {
                e.stopPropagation();
                remove(att);
              }}
            >
              <Trash2 className="h-3 w-3" />
            </Button>
          )}
        </span>
      ))}
      <Dialog
        open={view !== null}
        onClose={() => setView(null)}
        title={shown?.fileName ?? ""}
        closeLabel={t("ui.close")}
      >
        {shown && view !== null && (
          <>
            <AttachmentView att={shown} />
            {mine.length > 1 && (
              <div className="mt-3 flex items-center justify-between">
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label="‹"
                  onClick={() => setView((view + mine.length - 1) % mine.length)}
                >
                  <ChevronLeft className="h-4 w-4" />
                </Button>
                <span className="text-xs text-muted-foreground">
                  {view + 1} / {mine.length}
                </span>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label="›"
                  onClick={() => setView((view + 1) % mine.length)}
                >
                  <ChevronRight className="h-4 w-4" />
                </Button>
              </div>
            )}
          </>
        )}
      </Dialog>
    </div>
  );
}

/** Compact first-photo marker for the transactions list (+N when there are more). */
export function TxThumb({ txId }: { txId: string }) {
  const mine = useAttachments(txId);
  const first = mine[0];
  if (!first) return null;
  return (
    <span className="relative inline-flex shrink-0">
      <AttachmentThumb att={first} className="h-7 w-7 rounded-md" />
      {mine.length > 1 && (
        <span className="absolute -right-1.5 -top-1.5 rounded-full bg-primary px-1 text-[10px] font-semibold leading-4 text-primary-foreground">
          +{mine.length - 1}
        </span>
      )}
    </span>
  );
}
