"use client"

import type React from "react"

import { Zap, Shield, Globe, ImageIcon, Hash, Bell, Search, TrendingUp } from "lucide-react"
import { Card } from "@/components/ui/card"

export function LandingFeatures() {
  const features = [
    {
      icon: <Zap className="w-6 h-6" />,
      title: "Real-time Updates",
      description: "Experience instant notifications and live feed updates powered by cutting-edge technology.",
      gradient: "from-primary to-accent",
    },
    {
      icon: <Shield className="w-6 h-6" />,
      title: "Privacy First",
      description: "Your data is encrypted and secure. Control who sees your content with granular privacy settings.",
      gradient: "from-accent to-primary",
    },
    {
      icon: <ImageIcon className="w-6 h-6" />,
      title: "Rich Media",
      description: "Share photos, videos, and more with lightning-fast uploads and beautiful presentation.",
      gradient: "from-primary to-accent",
    },
    {
      icon: <Hash className="w-6 h-6" />,
      title: "Trending Topics",
      description: "Discover what the world is talking about with smart trending algorithms.",
      gradient: "from-accent to-primary",
    },
    {
      icon: <Bell className="w-6 h-6" />,
      title: "Smart Notifications",
      description: "Never miss what matters. Get personalized alerts that adapt to your interests.",
      gradient: "from-primary to-accent",
    },
    {
      icon: <Search className="w-6 h-6" />,
      title: "Advanced Search",
      description: "Find anything instantly with powerful search and filtering capabilities.",
      gradient: "from-accent to-primary",
    },
    {
      icon: <Globe className="w-6 h-6" />,
      title: "Global Reach",
      description: "Connect with people from every corner of the world in multiple languages.",
      gradient: "from-primary to-accent",
    },
    {
      icon: <TrendingUp className="w-6 h-6" />,
      title: "Analytics",
      description: "Track your impact with detailed insights about your posts and audience.",
      gradient: "from-accent to-primary",
    },
  ]

  return (
    <section id="features" className="py-24 relative overflow-hidden">
      <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_center,_var(--tw-gradient-stops))] from-primary/5 via-transparent to-transparent" />

      <div className="container mx-auto px-4 relative z-10">
        <div className="text-center mb-16">
          <div className="inline-block px-4 py-2 rounded-full bg-primary/10 border border-primary/20 text-primary text-sm font-medium mb-4">
            Features
          </div>
          <h2 className="text-4xl md:text-5xl font-bold mb-4 text-balance">
            Everything you need to
            <br />
            <span className="bg-gradient-to-r from-primary to-accent bg-clip-text text-transparent">
              stay connected
            </span>
          </h2>
          <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
            Packed with powerful features designed to make your social experience seamless and enjoyable.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
          {features.map((feature, index) => (
            <FeatureCard key={index} {...feature} delay={index * 50} />
          ))}
        </div>
      </div>
    </section>
  )
}

function FeatureCard({
  icon,
  title,
  description,
  gradient,
  delay,
}: {
  icon: React.ReactNode
  title: string
  description: string
  gradient: string
  delay: number
}) {
  return (
    <Card
      className="p-6 bg-card/50 border-border/50 backdrop-blur-sm hover-lift group transition-all duration-300 hover:border-primary/50"
      style={{ animationDelay: `${delay}ms` }}
    >
      <div
        className={`inline-flex items-center justify-center w-12 h-12 rounded-xl bg-gradient-to-br ${gradient} bg-opacity-10 text-primary mb-4 group-hover:scale-110 transition-transform duration-300`}
      >
        {icon}
      </div>
      <h3 className="text-lg font-semibold mb-2 group-hover:text-primary transition-colors">{title}</h3>
      <p className="text-sm text-muted-foreground leading-relaxed">{description}</p>
    </Card>
  )
}
