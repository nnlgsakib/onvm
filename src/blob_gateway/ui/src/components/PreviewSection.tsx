import { motion } from "framer-motion";

interface PreviewSectionProps {
  content: string | null;
  contentType?: string | null;
}

export function PreviewSection({ content, contentType }: PreviewSectionProps) {
  if (!content) return null;
  const safeType = contentType || "application/octet-stream";

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.5 }}
      className="relative rounded-2xl overflow-hidden border border-border/50 shadow-glow"
    >
      {/* Glow effect behind content */}
      <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-secondary/5" />
      
      {safeType.startsWith("image/") && (
        <img
          src={content}
          alt="Preview"
          className="relative w-full max-h-[500px] object-contain"
        />
      )}

      {safeType.startsWith("video/") && (
        <video
          src={content}
          controls
          className="relative w-full max-h-[500px]"
        />
      )}

      {safeType.startsWith("audio/") && (
        <div className="relative p-8 bg-muted/30">
          <audio src={content} controls className="w-full" />
        </div>
      )}

      {!safeType.startsWith("image/") &&
        !safeType.startsWith("video/") &&
        !safeType.startsWith("audio/") && (
          <div className="relative p-8 text-center text-muted-foreground">
            Preview not available for this content type
          </div>
        )}
    </motion.div>
  );
}
