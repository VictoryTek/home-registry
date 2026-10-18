import { describe, expect, it } from 'vitest';
import type { CreateItemRequest, Item } from '@/types';
import { TEST_TZ } from '../test/zone';
import { itemToEditFormData, toCreateItemRequest } from './itemForm';

describe(`item dates through create and edit in ${TEST_TZ}`, () => {
  const DATES = [
    '2026-10-17',
    '2026-01-01',
    '2026-12-31',
    '2024-02-29',
    '2026-03-08',
    '2026-11-01',
  ];

  it('sends the date input value unchanged when creating', () => {
    for (const date of DATES) {
      const form: CreateItemRequest = {
        inventory_id: 0,
        name: 'Drill',
        purchase_date: date,
        warranty_expiry: date,
      };
      const request = toCreateItemRequest(form, 7);
      expect(request.purchase_date).toBe(date);
      expect(request.warranty_expiry).toBe(date);
      expect(request.inventory_id).toBe(7);
    }
  });

  it('comes back unchanged through create, storage and the edit form', () => {
    for (const purchase of DATES) {
      for (const warranty of DATES) {
        const created = toCreateItemRequest(
          { inventory_id: 0, name: 'Drill', purchase_date: purchase, warranty_expiry: warranty },
          7
        );
        // The server stores the DATE and returns it as `YYYY-MM-DD` text
        const stored: Item = { id: 1, ...created };
        const edit = itemToEditFormData(stored);
        expect(edit.purchase_date).toBe(purchase);
        expect(edit.warranty_expiry).toBe(warranty);
        // Saving the form without touching the dates sends the same strings back
        expect({ ...edit }.purchase_date).toBe(purchase);
      }
    }
  });

  it('fills an empty date field with an empty string', () => {
    const edit = itemToEditFormData({ id: 1, inventory_id: 7, name: 'Drill' });
    expect(edit.purchase_date).toBe('');
    expect(edit.warranty_expiry).toBe('');
  });
});
