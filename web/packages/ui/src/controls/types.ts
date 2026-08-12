export type ControlSize = "sm" | "md" | "lg";
export type ControlStatus = "default" | "success" | "warning" | "danger";

export type ControlStyleProps = {
  size?: ControlSize;
  status?: ControlStatus;
  className?: string;
};
