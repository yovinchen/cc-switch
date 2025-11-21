import { useEffect, useState } from "react";

interface IconProps {
  size?: number;
  className?: string;
}

const LOBE_ICONS_VERSION = "latest"; // pin if needed, e.g. "1.4.0"
const LOBE_BASE = `https://cdn.jsdelivr.net/npm/@lobehub/icons-static-svg@${LOBE_ICONS_VERSION}/icons`;

function IconImage({
  urls,
  alt,
  size,
  className,
}: {
  urls: string[];
  alt: string;
  size: number;
  className?: string;
}) {
  const [index, setIndex] = useState(0);

  useEffect(() => {
    setIndex(0);
  }, [urls.join("|")]);

  const src = urls[index] ?? urls[urls.length - 1];

  return (
    <img
      src={src}
      width={size}
      height={size}
      className={className}
      alt={alt}
      loading="lazy"
      referrerPolicy="no-referrer"
      onError={() => {
        if (index < urls.length - 1) {
          setIndex((i) => i + 1);
        }
      }}
    />
  );
}

export function ClaudeIcon({ size = 16, className = "" }: IconProps) {
  return (
    <IconImage
      urls={[`${LOBE_BASE}/claude-color.svg`, `${LOBE_BASE}/claude.svg`]}
      size={size}
      className={className}
      alt="Claude"
    />
  );
}

export function CodexIcon({ size = 16, className = "" }: IconProps) {
  return (
    <IconImage
      urls={[
        `${LOBE_BASE}/openai-color.svg`,
        `${LOBE_BASE}/chatgpt-color.svg`,
        `${LOBE_BASE}/openai.svg`,
        `${LOBE_BASE}/chatgpt.svg`,
      ]}
      size={size}
      className={className}
      alt="Codex"
    />
  );
}

export function GeminiIcon({ size = 16, className = "" }: IconProps) {
  return (
    <IconImage
      urls={[`${LOBE_BASE}/gemini-color.svg`, `${LOBE_BASE}/gemini.svg`]}
      size={size}
      className={className}
      alt="Gemini"
    />
  );
}
