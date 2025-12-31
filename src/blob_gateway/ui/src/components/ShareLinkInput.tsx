import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Copy, Check } from "lucide-react";
import { toast } from "sonner";
import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface ShareLinkInputProps {
  value: string;
  label?: string;
}

export function ShareLinkInput({ value, label }: ShareLinkInputProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(value);
    setCopied(true);
    toast.success("Copied to clipboard!");
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="flex gap-3">
      <Input
        readOnly
        value={value}
        className="flex-1 text-sm bg-muted/30"
        aria-label={label}
      />
      <Button variant="copy" size="sm" onClick={handleCopy} className="min-w-[100px]">
        <AnimatePresence mode="wait">
          {copied ? (
            <motion.div
              key="check"
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              exit={{ scale: 0 }}
              className="flex items-center gap-2"
            >
              <Check className="h-4 w-4" />
              Copied
            </motion.div>
          ) : (
            <motion.div
              key="copy"
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              exit={{ scale: 0 }}
              className="flex items-center gap-2"
            >
              <Copy className="h-4 w-4" />
              Copy
            </motion.div>
          )}
        </AnimatePresence>
      </Button>
    </div>
  );
}
