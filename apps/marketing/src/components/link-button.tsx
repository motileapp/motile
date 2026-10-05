import type { VariantProps } from "class-variance-authority"
import type { ComponentProps } from "react"
import { buttonVariants } from "@/components/ui/button"
import { cn } from "@/lib/utils"

export function LinkButton({
  className,
  variant = "default",
  size = "default",
  ...props
}: ComponentProps<"a"> & VariantProps<typeof buttonVariants>) {
  return (
    <a
      data-slot="button"
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  )
}
