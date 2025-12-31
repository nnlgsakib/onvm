import { motion } from "framer-motion";
import { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

interface StatCardProps {
  icon: LucideIcon;
  label: string;
  value: string;
  delay?: number;
}

export function StatCard({ icon: Icon, label, value, delay = 0 }: StatCardProps) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay, duration: 0.4 }}
      className="group relative overflow-hidden rounded-xl p-5 glass border border-border/50 hover:border-primary/30 transition-all duration-300"
    >
      {/* Glow on hover */}
      <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-300" />
      
      <div className="relative flex items-start justify-between">
        <div>
          <p className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-1">
            {label}
          </p>
          <p className="text-lg font-mono font-semibold text-foreground">
            {value}
          </p>
        </div>
        <div className="p-2 rounded-lg bg-primary/10 text-primary group-hover:shadow-glow-sm transition-all duration-300">
          <Icon className="w-5 h-5" />
        </div>
      </div>
    </motion.div>
  );
}
