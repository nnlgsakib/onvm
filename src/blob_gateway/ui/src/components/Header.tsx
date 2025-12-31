import { motion } from "framer-motion";
import { Globe, Zap, Shield, Database } from "lucide-react";

export function Header() {
  return (
    <motion.div
      initial={{ opacity: 0, y: -20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.6 }}
      className="text-center mb-10"
    >
      {/* Logo */}
      <motion.div
        className="inline-flex items-center justify-center w-20 h-20 rounded-2xl gradient-bg shadow-glow mb-6"
        animate={{ 
          boxShadow: [
            "0 0 40px hsla(199, 89%, 48%, 0.3)",
            "0 0 60px hsla(199, 89%, 48%, 0.5)",
            "0 0 40px hsla(199, 89%, 48%, 0.3)",
          ]
        }}
        transition={{ duration: 2, repeat: Infinity }}
      >
        <Globe className="w-10 h-10 text-primary-foreground" />
      </motion.div>

      {/* Title */}
      <h1 className="text-4xl md:text-5xl font-bold mb-3">
        <span className="gradient-text glow-text">ONVM</span>
        <span className="text-foreground ml-3">Blob Gateway</span>
      </h1>

      {/* Subtitle */}
      <p className="text-lg text-muted-foreground mb-6">
        Decentralized Content Delivery Network
      </p>

      {/* Feature badges */}
      <div className="flex flex-wrap items-center justify-center gap-3">
        {[
          { icon: Zap, label: "Lightning Fast" },
          { icon: Shield, label: "Secure" },
          { icon: Database, label: "Distributed" },
        ].map((feature, i) => (
          <motion.div
            key={feature.label}
            initial={{ opacity: 0, scale: 0.8 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ delay: 0.3 + i * 0.1 }}
            className="flex items-center gap-2 px-4 py-2 rounded-full glass border border-border/50 text-sm text-muted-foreground"
          >
            <feature.icon className="w-4 h-4 text-primary" />
            {feature.label}
          </motion.div>
        ))}
      </div>
    </motion.div>
  );
}
