import type { AppId } from "@/lib/api";
import { ClaudeIcon, CodexIcon, GeminiIcon } from "./BrandIcons";

interface AppSwitcherProps {
  activeApp: AppId;
  onSwitch: (app: AppId) => void;
}

export function AppSwitcher({ activeApp, onSwitch }: AppSwitcherProps) {
  const handleSwitch = (app: AppId) => {
    if (app === activeApp) return;
    onSwitch(app);
  };

  return (
    <div className="glass p-1.5 rounded-full flex items-center gap-1.5">
      <button
        type="button"
        onClick={() => handleSwitch("claude")}
        className={`group relative flex items-center gap-2 px-4 py-2.5 rounded-full text-sm font-semibold overflow-hidden transition-all duration-300 ease-out ${
          activeApp === "claude"
            ? "text-white scale-[1.02] shadow-[0_12px_35px_-15px_rgba(249,115,22,0.8)] ring-1 ring-white/10"
            : "text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
        }`}
      >
        {activeApp === "claude" && (
          <div className="absolute inset-0 bg-gradient-to-r from-orange-500 via-amber-500 to-red-600 rounded-full opacity-90 blur-[1px] transition-all duration-500 -z-10 scale-100" />
        )}
        {activeApp !== "claude" && (
          <div className="absolute inset-0 rounded-full bg-white/0 transition-all duration-300 -z-10" />
        )}
        <ClaudeIcon
          size={16}
          className={
            activeApp === "claude"
              ? "text-white"
              : "text-muted-foreground group-hover:text-orange-500 transition-colors"
          }
        />
        <span>Claude</span>
      </button>

      <button
        type="button"
        onClick={() => handleSwitch("codex")}
        className={`group relative flex items-center gap-2 px-4 py-2.5 rounded-full text-sm font-semibold overflow-hidden transition-all duration-300 ease-out ${
          activeApp === "codex"
            ? "text-white scale-[1.02] shadow-[0_12px_35px_-15px_rgba(59,130,246,0.8)] ring-1 ring-white/10"
            : "text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
        }`}
      >
        {activeApp === "codex" && (
          <div className="absolute inset-0 bg-gradient-to-r from-blue-500 via-sky-500 to-cyan-500 rounded-full opacity-90 blur-[1px] transition-all duration-500 -z-10 scale-100" />
        )}
        {activeApp !== "codex" && (
          <div className="absolute inset-0 rounded-full bg-white/0 transition-all duration-300 -z-10" />
        )}
        <CodexIcon
          size={16}
          className={
            activeApp === "codex"
              ? "text-white"
              : "text-muted-foreground group-hover:text-blue-500 transition-colors"
          }
        />
        <span>Codex</span>
      </button>

      <button
        type="button"
        onClick={() => handleSwitch("gemini")}
        className={`group relative flex items-center gap-2 px-4 py-2.5 rounded-full text-sm font-semibold overflow-hidden transition-all duration-300 ease-out ${
          activeApp === "gemini"
            ? "text-white scale-[1.02] shadow-[0_12px_35px_-15px_rgba(99,102,241,0.8)] ring-1 ring-white/10"
            : "text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
        }`}
      >
        {activeApp === "gemini" && (
          <div className="absolute inset-0 bg-gradient-to-r from-indigo-500 via-violet-500 to-purple-600 rounded-full opacity-90 blur-[1px] transition-all duration-500 -z-10 scale-100" />
        )}
        {activeApp !== "gemini" && (
          <div className="absolute inset-0 rounded-full bg-white/0 transition-all duration-300 -z-10" />
        )}
        <GeminiIcon
          size={16}
          className={
            activeApp === "gemini"
              ? "text-white"
              : "text-muted-foreground group-hover:text-indigo-500 transition-colors"
          }
        />
        <span>Gemini</span>
      </button>
    </div>
  );
}
