import { motion } from "framer-motion";

interface PreviewSectionProps {
  content: string | null;
  contentType: string;
}

export function PreviewSection({ content, contentType }: PreviewSectionProps) {
  if (!content) return null;

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.5 }}
      className="relative rounded-2xl overflow-hidden border border-border/50 shadow-glow"
    >
      {/* Glow effect behind content */}
      <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-secondary/5" />
      
      {contentType.startsWith("image/") && (
        <img
          src={content}
          alt="Preview"
          className="relative w-full max-h-[500px] object-contain"
        />
      )}

      {contentType.startsWith("video/") && (
        <video
          src={content}
          controls
          className="relative w-full max-h-[500px]"
        />
      )}

      {contentType.startsWith("audio/") && (
        <div className="relative p-8 bg-muted/30">
          <audio src={content} controls className="w-full" />
        </div>
      )}

      {!contentType.startsWith("image/") &&
        !contentType.startsWith("video/") &&
        !contentType.startsWith("audio/") && (
          <div className="relative p-8 text-center text-muted-foreground">
            Preview not available for this content type
          </div>
        )}
    </motion.div>
  );
}
