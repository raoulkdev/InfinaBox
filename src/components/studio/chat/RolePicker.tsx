import type { Role } from "@/lib/studio-types";

// Wave 0 stub (Phase C plan, Task FP): "Ask as: Auto / Designer / …" in the
// chat composer. The lead wires it into ChatComposer.

export interface RolePickerProps {
  value: Role;
  onChange: (role: Role) => void;
  disabled?: boolean;
}

export function RolePicker(_props: RolePickerProps) {
  return null;
}
