import { useParams, Link } from "react-router-dom";
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { AnimatedBackground } from "@/components/AnimatedBackground";
import { GlassCard } from "@/components/GlassCard";
import { StatCard } from "@/components/StatCard";
import { ShareLinkInput } from "@/components/ShareLinkInput";
import { PreviewSection } from "@/components/PreviewSection";
import { Spinner } from "@/components/Spinner";
import { Button } from "@/components/ui/button";
import { formatSize, truncateId } from "@/lib/format";
import { toast } from "sonner";
import {
  ArrowLeft,
  Download,
  Copy,
  Code,
  Globe,
  Hash,
  FileType,
  HardDrive,
} from "lucide-react";

interface BlobInfo {
  id: string;
  size: number;
  chunk_count: number;
  subchunk_count: number;
  mime_type: string | null;
  detected_mime_type: string | null;
  available_locally: boolean;
}

export default function SharePage() {
  const { id } = useParams<{ id: string }>();
  const [loading, setLoading] = useState(true);
  const [blobInfo, setBlobInfo] = useState<BlobInfo | null>(null);
  const contentId = id || "";
  const apiBase = "/explorer/api";
  const cdnBase = "/explorer/api/cdn";

  useEffect(() => {
    const fetchBlobInfo = async () => {
      if (!contentId) return;

      try {
        const response = await fetch(`${apiBase}/blob/${contentId}/info`);
        if (!response.ok) {
          throw new Error(await response.text());
        }

        const info = await response.json();
        setBlobInfo(info);
      } catch (error: any) {
        toast.error(`Error: ${error.message}`);
      } finally {
        setLoading(false);
      }
    };

    fetchBlobInfo();
  }, [contentId]);

  const directLink = `${window.location.origin}${cdnBase}/${contentId}`;
  const embedCode = `<img src="${directLink}" alt="ONVM Content">`;

  const handleCopyDirectLink = () => {
    navigator.clipboard.writeText(directLink);
    toast.success("Direct link copied to clipboard!");
  };

  const handleCopyEmbedCode = () => {
    navigator.clipboard.writeText(embedCode);
    toast.success("Embed code copied to clipboard!");
  };

  if (loading) {
    return (
      <div className="min-h-screen relative">
        <AnimatedBackground />
        <div className="relative z-10 min-h-screen flex items-center justify-center">
          <Spinner />
        </div>
      </div>
    );
  }

  if (!blobInfo) {
    return (
      <div className="min-h-screen relative">
        <AnimatedBackground />
        <div className="relative z-10 min-h-screen flex items-center justify-center">
          <GlassCard>
            <div className="text-center">
              <p className="text-xl text-muted-foreground">Blob not found</p>
              <Link to="/" className="mt-4 inline-block text-primary hover:underline">
                Back to Explorer
              </Link>
            </div>
          </GlassCard>
        </div>
      </div>
    );
  }

  const mimeType = blobInfo.detected_mime_type || blobInfo.mime_type || "application/octet-stream";

  return (
    <div className="min-h-screen relative">
      <AnimatedBackground />

      <div className="relative z-10 min-h-screen p-4 md:p-8">
        <div className="max-w-4xl mx-auto">
          {/* Back Button */}
          <motion.div
            initial={{ opacity: 0, x: -20 }}
            animate={{ opacity: 1, x: 0 }}
          >
            <Link
              to="/"
              className="inline-flex items-center gap-2 text-muted-foreground hover:text-foreground mb-8 transition-colors group"
            >
              <ArrowLeft className="h-4 w-4 group-hover:-translate-x-1 transition-transform" />
              Back to Explorer
            </Link>
          </motion.div>

          <GlassCard glow>
            {/* Header */}
            <motion.div
              initial={{ opacity: 0, y: -20 }}
              animate={{ opacity: 1, y: 0 }}
              className="text-center mb-8"
            >
              <motion.div
                className="inline-flex items-center justify-center w-16 h-16 rounded-2xl gradient-bg shadow-glow mb-5"
                animate={{
                  boxShadow: [
                    "0 0 40px hsla(199, 89%, 48%, 0.3)",
                    "0 0 60px hsla(199, 89%, 48%, 0.5)",
                    "0 0 40px hsla(199, 89%, 48%, 0.3)",
                  ],
                }}
                transition={{ duration: 2, repeat: Infinity }}
              >
                <Globe className="w-8 h-8 text-primary-foreground" />
              </motion.div>

              <h1 className="text-3xl md:text-4xl font-bold mb-2">
                <span className="gradient-text">Shared</span>
                <span className="text-foreground ml-2">Content</span>
              </h1>
              <p className="text-muted-foreground">
                Decentralized Content Delivery Network
              </p>
            </motion.div>

            {/* Stats */}
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 mb-8">
              <StatCard
                icon={Hash}
                label="Content ID"
                value={truncateId(contentId, 12)}
                delay={0.1}
              />
              <StatCard
                icon={FileType}
                label="Type"
                value={mimeType}
                delay={0.2}
              />
              <StatCard
                icon={HardDrive}
                label="Size"
                value={formatSize(blobInfo.size)}
                delay={0.3}
              />
            </div>

            {/* Preview */}
            <motion.div
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              transition={{ delay: 0.4 }}
              className="mb-8"
            >
              <PreviewSection
                content={directLink}
                contentType={mimeType}
              />
            </motion.div>

            {/* Action Buttons */}
            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.5 }}
              className="flex flex-wrap gap-3 mb-8"
            >
              <Button asChild variant="glow" size="lg">
                <a href={directLink} download>
                  <Download className="h-5 w-5" />
                  Download
                </a>
              </Button>
              <Button onClick={handleCopyDirectLink} variant="secondary" size="lg">
                <Copy className="h-5 w-5" />
                Copy Direct Link
              </Button>
              <Button onClick={handleCopyEmbedCode} variant="secondary" size="lg">
                <Code className="h-5 w-5" />
                Copy Embed Code
              </Button>
            </motion.div>

            {/* Share Links */}
            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.6 }}
              className="rounded-xl p-6 glass border border-border/50"
            >
              <h3 className="font-semibold text-foreground mb-4 flex items-center gap-2">
                <div className="w-2 h-2 rounded-full bg-primary animate-pulse" />
                Share Links
              </h3>
              <div className="space-y-4">
                <div>
                  <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-2 block">
                    Direct Link
                  </label>
                  <ShareLinkInput value={directLink} label="Direct link" />
                </div>
                <div>
                  <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-2 block">
                    Embed Code
                  </label>
                  <ShareLinkInput value={embedCode} label="Embed code" />
                </div>
              </div>
            </motion.div>
          </GlassCard>

          {/* Footer */}
          <motion.p
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.8 }}
            className="text-center text-sm text-muted-foreground mt-6"
          >
            Powered by ONVM Decentralized Network
          </motion.p>
        </div>
      </div>
    </div>
  );
}
