import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2.5 whitespace-nowrap font-semibold transition-all duration-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-5 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default:
          "gradient-bg text-primary-foreground rounded-xl hover:shadow-glow hover:scale-[1.02] active:scale-[0.98]",
        secondary:
          "glass rounded-xl text-foreground hover:bg-muted/50 hover:shadow-glow-sm border border-border",
        outline:
          "gradient-border bg-transparent text-foreground rounded-xl hover:bg-muted/30",
        ghost: 
          "text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded-xl",
        destructive:
          "bg-destructive text-destructive-foreground rounded-xl hover:bg-destructive/90",
        link: 
          "text-primary underline-offset-4 hover:underline",
        glow:
          "gradient-bg text-primary-foreground rounded-xl shadow-glow animate-pulse-glow hover:scale-[1.02] active:scale-[0.98]",
        copy:
          "bg-primary/20 text-primary border border-primary/30 rounded-lg hover:bg-primary/30 hover:shadow-glow-sm",
      },
      size: {
        default: "h-12 px-6 text-sm",
        sm: "h-10 px-4 text-sm",
        lg: "h-14 px-8 text-base",
        xl: "h-16 px-10 text-lg",
        icon: "h-10 w-10 rounded-lg",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  }
);
Button.displayName = "Button";

export { Button, buttonVariants };
