import { Label } from "@/components/ui/label"
import { RadioGroupItem } from "@/components/ui/radio-group"

interface EndpointOptionProps {
  id: string
  value: string
  label: string
}

export function EndpointOption({ id, value, label }: EndpointOptionProps) {
  return (
    <div className="flex items-center gap-2">
      <RadioGroupItem id={id} value={value} />
      <Label htmlFor={id} className="min-w-0 cursor-pointer break-all text-sm font-normal text-muted-foreground">
        {label}
      </Label>
    </div>
  )
}
