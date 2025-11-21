import React from "react";
import { createPortal } from "react-dom";
import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui/button";

interface FullScreenPanelProps {
  isOpen: boolean;
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  footer?: React.ReactNode;
}

/**
 * Reusable full-screen panel component
 * Handles portal rendering, header with back button, and footer
 * Uses solid theme colors without transparency
 */
export const FullScreenPanel: React.FC<FullScreenPanelProps> = ({
  isOpen,
  title,
  onClose,
  children,
  footer,
}) => {
  if (!isOpen) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-[60] flex flex-col"
      style={{ backgroundColor: "hsl(var(--background))" }}
    >
      {/* Header */}
      <div
        className="flex-shrink-0 py-4 border-b border-border-default"
        style={{ backgroundColor: "hsl(var(--background))" }}
      >
        <div className="mx-auto max-w-[60rem] px-6 flex items-center gap-4">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={onClose}
            className="hover:bg-black/5 dark:hover:bg-white/5"
          >
            <ArrowLeft className="h-5 w-5" />
          </Button>
          <h2 className="text-lg font-semibold text-foreground">{title}</h2>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[60rem] px-6 py-6 space-y-6 w-full">
          {children}
        </div>
      </div>

      {/* Footer */}
      {footer && (
        <div
          className="flex-shrink-0 py-4 border-t border-border-default"
          style={{ backgroundColor: "hsl(var(--background))" }}
        >
          <div className="mx-auto max-w-[60rem] px-6 flex items-center justify-end gap-3">
            {footer}
          </div>
        </div>
      )}
    </div>,
    document.body,
  );
};
