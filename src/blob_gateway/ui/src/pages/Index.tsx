import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { motion, AnimatePresence } from "framer-motion";
import { AnimatedBackground } from "@/components/AnimatedBackground";
import { GlassCard } from "@/components/GlassCard";
import { Header } from "@/components/Header";
import { StatCard } from "@/components/StatCard";
import { Spinner } from "@/components/Spinner";
import { PreviewSection } from "@/components/PreviewSection";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { formatSize, truncateId } from "@/lib/format";
import { toast } from "sonner";
import {
  Search,
  Eye,
  Download,
  Share2,
  Link2,
  Hash,
  HardDrive,
  Layers,
  FileType,
  CheckCircle2,
} from "lucide-react";

interface BlobInfo {
  id: string;
  size: number;
  chunk_count: number;
  subchunk_count: number;
  mime_type: string;
  detected_mime_type: string;
  available_locally: boolean;
}

const DEMO_BLOB: BlobInfo = {
  id: "a1b2c3d4e5f67890abcdef1234567890",
  size: 2516582,
  chunk_count: 4,
  subchunk_count: 16,
  mime_type: "image/png",
  detected_mime_type: "image/png",
  available_locally: true,
};

export default function Explorer() {
  const navigate = useNavigate();
  const [blobId, setBlobId] = useState("");
  const [loading, setLoading] = useState(false);
  const [blobInfo, setBlobInfo] = useState<BlobInfo | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);

  const fetchBlobInfo = async () => {
    if (!blobId.trim()) {
      toast.error("Please enter a Blob ID");
      return;
    }

    setLoading(true);
    setBlobInfo(null);
    setPreviewUrl(null);

    try {
      const response = await fetch(`/api/blob/${blobId}/info`);
      if (!response.ok) {
        throw new Error(await response.text());
      }

      const info = await response.json();
      setBlobInfo(info);
      toast.success("Blob info loaded successfully");
    } catch (error: any) {
      toast.error(`Error: ${error.message}`);
    } finally {
      setLoading(false);
    }
  };

  const viewBlob = async () => {
    if (!blobInfo) return;

    setLoading(true);
    try {
      const cdnUrl = `/cdn/${blobInfo.id}`;
      setPreviewUrl(cdnUrl);
      toast.success("Blob loaded successfully");
    } catch (error) {
      toast.error("Error loading blob");
    } finally {
      setLoading(false);
    }
  };

  const downloadBlob = async () => {
    if (!blobInfo) return;

    setLoading(true);
    try {
      const response = await fetch(`/api/blob/${blobInfo.id}?download=true`);
      if (!response.ok) {
        throw new Error(await response.text());
      }

      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;

      const disposition = response.headers.get("content-disposition");
      let filename = "download";
      if (disposition) {
        const match = disposition.match(/filename="([^"]+)"/);
        if (match) filename = match[1];
      }
      a.download = filename;

      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);

      toast.success("Download started");
    } catch (error: any) {
      toast.error(`Error: ${error.message}`);
    } finally {
      setLoading(false);
    }
  };

  const shareBlob = () => {
    if (!blobInfo) {
      toast.error("Please fetch blob info first");
      return;
    }
    navigate(`/share/${blobInfo.id}`);
  };

  const copyDirectLink = () => {
    if (!blobInfo) {
      toast.error("Please fetch blob info first");
      return;
    }
    const cdnUrl = `${window.location.origin}/cdn/${blobInfo.id}`;
    navigator.clipboard.writeText(cdnUrl);
    toast.success("CDN link copied to clipboard!");
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      fetchBlobInfo();
    }
  };

  return (
    <div className="min-h-screen relative">
      <AnimatedBackground />

      <div className="relative z-10 min-h-screen flex items-center justify-center p-4 md:p-8">
        <div className="w-full max-w-3xl">
          <Header />

          <GlassCard glow>
            {/* Search Section */}
            <div className="flex flex-col sm:flex-row gap-3 mb-8">
              <div className="relative flex-1">
                <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground" />
                <Input
                  type="text"
                  placeholder="Enter Blob ID (hex)"
                  value={blobId}
                  onChange={(e) => setBlobId(e.target.value)}
                  onKeyDown={handleKeyDown}
                  className="pl-12"
                />
              </div>
              <Button
                onClick={fetchBlobInfo}
                disabled={loading}
                size="lg"
                className="sm:w-auto"
              >
                <Search className="w-5 h-5" />
                Fetch Info
              </Button>
            </div>

            <AnimatePresence mode="wait">
              {loading && (
                <motion.div
                  key="loading"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                >
                  <Spinner />
                </motion.div>
              )}

              {blobInfo && !loading && (
                <motion.div
                  key="content"
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -20 }}
                  className="space-y-6"
                >
                  {/* Stats Grid */}
                  <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
                    <StatCard
                      icon={Hash}
                      label="Content ID"
                      value={truncateId(blobInfo.id, 12)}
                      delay={0}
                    />
                    <StatCard
                      icon={HardDrive}
                      label="Size"
                      value={formatSize(blobInfo.size)}
                      delay={0.1}
                    />
                    <StatCard
                      icon={Layers}
                      label="Chunks"
                      value={`${blobInfo.chunk_count} / ${blobInfo.subchunk_count}`}
                      delay={0.2}
                    />
                    <StatCard
                      icon={FileType}
                      label="MIME Type"
                      value={blobInfo.mime_type || "Unknown"}
                      delay={0.3}
                    />
                    <StatCard
                      icon={FileType}
                      label="Detected"
                      value={blobInfo.detected_mime_type || "N/A"}
                      delay={0.4}
                    />
                    <StatCard
                      icon={CheckCircle2}
                      label="Available"
                      value={blobInfo.available_locally ? "Local ✓" : "Remote"}
                      delay={0.5}
                    />
                  </div>

                  {/* Action Buttons */}
                  <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    transition={{ delay: 0.6 }}
                    className="grid grid-cols-2 md:grid-cols-4 gap-3"
                  >
                    <Button onClick={viewBlob} variant="glow">
                      <Eye className="w-5 h-5" />
                      View
                    </Button>
                    <Button onClick={downloadBlob}>
                      <Download className="w-5 h-5" />
                      Download
                    </Button>
                    <Button onClick={shareBlob} variant="secondary">
                      <Share2 className="w-5 h-5" />
                      Share
                    </Button>
                    <Button onClick={copyDirectLink} variant="secondary">
                      <Link2 className="w-5 h-5" />
                      CDN Link
                    </Button>
                  </motion.div>

                  {/* Preview */}
                  {previewUrl && (
                    <PreviewSection
                      content={previewUrl}
                      contentType={blobInfo.mime_type}
                    />
                  )}
                </motion.div>
              )}

              {/* Empty State */}
              {!blobInfo && !loading && (
                <motion.div
                  key="empty"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  className="text-center py-12"
                >
                  <div className="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-muted/50 mb-4">
                    <Search className="w-8 h-8 text-muted-foreground" />
                  </div>
                  <p className="text-muted-foreground">
                    Enter a Blob ID to get started
                  </p>
                </motion.div>
              )}
            </AnimatePresence>
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