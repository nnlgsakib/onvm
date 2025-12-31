import { motion } from "framer-motion";
import { cn } from "@/lib/utils";

interface InfoItemProps {
  label: string;
  value: string;
  icon?: React.ReactNode;
  className?: string;
}

export function InfoItem({ label, value, icon, className }: InfoItemProps) {
  return (
    <motion.div
      initial={{ opacity: 0, x: -10 }}
      animate={{ opacity: 1, x: 0 }}
      className={cn(
        "flex items-center justify-between p-4 rounded-xl bg-muted/30 border border-border/50 hover:border-primary/30 transition-all duration-300 group",
        className
      )}
    >
      <div className="flex items-center gap-3">
        {icon && (
          <div className="text-muted-foreground group-hover:text-primary transition-colors">
            {icon}
          </div>
        )}
        <span className="text-sm font-medium text-muted-foreground">{label}</span>
      </div>
      <span className="font-mono text-sm text-foreground">{value}</span>
    </motion.div>
  );
}
