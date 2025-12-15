import { HeroSection } from "@/components/hero-section"
import { RegistrationForm } from "@/components/registration-form"
import { IdentityGallery } from "@/components/identity-gallery"
import { StatsSection } from "@/components/stats-section"

export default function HomePage() {
  return (
    <main className="min-h-screen bg-background">
      <HeroSection />
      <RegistrationForm />
      <IdentityGallery />
      <StatsSection />
    </main>
  )
}
