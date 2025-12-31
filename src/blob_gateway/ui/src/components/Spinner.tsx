import { motion } from "framer-motion";

export function Spinner() {
  return (
    <div className="flex flex-col items-center justify-center gap-4 py-12">
      <motion.div
        className="relative w-16 h-16"
        animate={{ rotate: 360 }}
        transition={{ duration: 2, repeat: Infinity, ease: "linear" }}
      >
        <div className="absolute inset-0 rounded-full border-2 border-primary/20" />
        <div className="absolute inset-0 rounded-full border-2 border-transparent border-t-primary shadow-glow" />
      </motion.div>
      <motion.p
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        className="text-sm text-muted-foreground font-medium"
      >
        Loading...
      </motion.p>
    </div>
  );
}
