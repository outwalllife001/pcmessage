export interface Device {
  id: string;
  name: string;
  port: number;
  certificate: string;
  version: number;
  platform: string;
}
export interface Peer extends Device {
  address: string;
  online: boolean;
  paired: boolean;
  unread: number;
}
export interface Attachment {
  id: string;
  name: string;
  mime: string;
  size: number;
  hash: string;
}
export interface Message {
  id: string;
  peer_id: string;
  text: string;
  images: Attachment[];
  created_at: number;
  direction: "incoming" | "outgoing";
  status: "sending" | "sent" | "failed";
  unread: boolean;
}
export interface Pairing {
  id: string;
  peer_id: string;
  name: string;
  code: string;
  incoming: boolean;
  confirmed: boolean;
}
export interface Snapshot {
  local: Device;
  addresses: string[];
  peers: Peer[];
  pairings: Pairing[];
  network_error: string | null;
}
export interface Draft {
  text: string;
  images: Attachment[];
}
