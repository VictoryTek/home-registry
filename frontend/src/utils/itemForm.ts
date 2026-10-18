/**
 * Mappings between the item form and the API for the date fields. The date inputs hold
 * `YYYY-MM-DD` strings and the API stores and returns `YYYY-MM-DD` text, so the strings pass
 * through untouched: a `Date` object (and with it the machine's time zone) never gets involved.
 */

import type { CreateItemRequest, Item, UpdateItemRequest } from '@/types';

/** The request body for creating an item from the add-item form. */
export function toCreateItemRequest(
  form: CreateItemRequest,
  inventoryId: number
): CreateItemRequest {
  return { ...form, inventory_id: inventoryId };
}

/** The edit form's initial values for an existing item. */
export function itemToEditFormData(item: Item): UpdateItemRequest {
  return {
    name: item.name,
    description: item.description ?? '',
    purchase_date: item.purchase_date ?? '',
    purchase_price: item.purchase_price,
    warranty_expiry: item.warranty_expiry ?? '',
    notes: item.notes ?? '',
    quantity: item.quantity ?? 1,
  };
}
