/**
 * Fixtures del demo del Agregador: 8 mensajes de demanda en lenguaje natural
 * (jerga chilena incluida) y un panel de productores sembrados con sus ofertas.
 * Los account-hash son válidos en formato; el primero es el productor real del
 * historial on-chain (para que `open_lote` registre una identidad conocida).
 */

import type { Offer } from "./types.js";

export interface RawDemand {
  readonly buyerId: string;
  readonly raw: string;
}

export const DEMANDS_RAW: readonly RawDemand[] = [
  { buyerId: "resto-cocina-01", raw: "unas 20 cajas de tomate para el jueves, que estén firmes, no pago más de 8 lucas la caja" },
  { buyerId: "resto-parrilla-02", raw: "necesito 15 cajas de tomate firme el jueves, tope 8500 la caja porfa" },
  { buyerId: "cafe-esquina-03", raw: "12 cajas de tomate firmes, jueves, hasta 9 lucas" },
  { buyerId: "bar-centro-04", raw: "quiero 10 cajas de lechuga estándar para el viernes" },
  { buyerId: "resto-veg-05", raw: "8 cajas de lechuga estándar el viernes, lo que sea razonable" },
  { buyerId: "hotel-plaza-06", raw: "30 cajas de tomate premium para el sábado, sin tope, que sea de primera" },
  { buyerId: "picada-07", raw: "unas 18 cajas de tomate firme el jueves, no más de 8 lucas" },
  { buyerId: "resto-mar-08", raw: "6 cajas de lechuga estándar viernes" },
];

/** Productores sembrados (nombre → account-hash). */
export const PRODUCERS: Readonly<Record<string, string>> = {
  "la-huerta": "account-hash-33518b62a4434cb640d6239c86e86f1ed1c132df9ddc2d1cf6f629913ad1f1ba",
  "agro-sur": "account-hash-11a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f80",
  "verde-fresco": "account-hash-99887766554433221100ffeeddccbbaa99887766554433221100ffeeddccbb",
};

/** Ofertas de los productores al RFQ (off-chain). */
export const OFFERS: readonly Offer[] = [
  // Tomate firme RM jueves — la-huerta el más barato (gana por precio)
  { producer: PRODUCERS["la-huerta"]!, producto: "tomate", calidad: "firme", zona: "RM", precioUnitario: 7800, disponible: 120 },
  { producer: PRODUCERS["agro-sur"]!, producto: "tomate", calidad: "firme", zona: "RM", precioUnitario: 8200, disponible: 90 },
  { producer: PRODUCERS["verde-fresco"]!, producto: "tomate", calidad: "firme", zona: "RM", precioUnitario: 8000, disponible: 60 },
  // Lechuga estándar RM viernes
  { producer: PRODUCERS["agro-sur"]!, producto: "lechuga", calidad: "estandar", zona: "RM", precioUnitario: 3200, disponible: 40 },
  { producer: PRODUCERS["verde-fresco"]!, producto: "lechuga", calidad: "estandar", zona: "RM", precioUnitario: 3500, disponible: 50 },
  // Tomate premium RM sábado
  { producer: PRODUCERS["la-huerta"]!, producto: "tomate", calidad: "premium", zona: "RM", precioUnitario: 12000, disponible: 40 },
];
