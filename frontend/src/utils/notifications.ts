/**
 * Notifications utility
 * Checks for warranty expirations and other alerts
 */

import type { Item, DismissedWarranties } from '@/types';
import { daysUntilDate } from './dateFormat';

export interface WarrantyNotification {
  id: number;
  itemName: string;
  inventoryId: number;
  warrantyExpiry: string;
  daysUntilExpiry: number;
  status: 'expired' | 'expiring-soon' | 'expiring-this-month';
}

/**
 * Check items for warranty expiry notifications
 * @param items - Array of items to check
 * @param dismissedWarranties - Map of dismissed notifications (itemId -> dismissal info)
 * @param daysThreshold - Days before expiry to consider "expiring soon" (default 30)
 * @returns Array of warranty notifications
 */
export function checkWarrantyNotifications(
  items: Item[],
  dismissedWarranties: DismissedWarranties = {},
  daysThreshold = 30
): WarrantyNotification[] {
  const notifications: WarrantyNotification[] = [];
  const now = new Date();

  items.forEach((item) => {
    if (!item.warranty_expiry || !item.id) {
      return;
    }

    // Enhancement 3: Check if dismissed
    const dismissed = dismissedWarranties[String(item.id)];
    if (dismissed) {
      // If warranty date hasn't changed, keep dismissed
      if (dismissed.warrantyExpiry === item.warranty_expiry) {
        return; // Skip this notification
      }
      // If warranty date changed, show notification again (user updated it)
    }

    // Calendar days from today: the expiry is a date with no time, so it is never turned into a Date
    const diffDays = daysUntilDate(item.warranty_expiry, now);
    if (diffDays === null) {
      return; // Not a valid date
    }

    // Determine status based on days until expiry
    let status: WarrantyNotification['status'];

    if (diffDays < 0) {
      // Warranty expired - only show if expired within the threshold
      const daysExpired = Math.abs(diffDays);
      if (daysExpired > daysThreshold) {
        return; // Expired too long ago - don't show badge
      }
      status = 'expired';
    } else if (diffDays <= 7) {
      status = 'expiring-soon';
    } else if (diffDays <= daysThreshold) {
      status = 'expiring-this-month';
    } else {
      return; // Not expiring soon enough to notify
    }

    notifications.push({
      id: item.id,
      itemName: item.name,
      inventoryId: item.inventory_id,
      warrantyExpiry: item.warranty_expiry,
      daysUntilExpiry: diffDays,
      status,
    });
  });

  // Sort by urgency: expired first, then by days until expiry
  return notifications.sort((a, b) => {
    if (a.status === 'expired' && b.status !== 'expired') {
      return -1;
    }
    if (a.status !== 'expired' && b.status === 'expired') {
      return 1;
    }
    return a.daysUntilExpiry - b.daysUntilExpiry;
  });
}

/**
 * Get notification message for a warranty notification
 */
export function getNotificationMessage(notification: WarrantyNotification): string {
  const { itemName, daysUntilExpiry, status } = notification;

  if (status === 'expired') {
    const daysExpired = Math.abs(daysUntilExpiry);
    return `${itemName} warranty expired ${daysExpired === 1 ? '1 day' : `${daysExpired} days`} ago`;
  } else if (status === 'expiring-soon') {
    return `${itemName} warranty expires in ${daysUntilExpiry === 1 ? '1 day' : `${daysUntilExpiry} days`}`;
  } else {
    return `${itemName} warranty expires in ${daysUntilExpiry} days`;
  }
}

/**
 * Whether a warranty has expired. A warranty is valid through its expiry date and expired from
 * the next day, the same rule `checkWarrantyNotifications` uses.
 * @param warrantyExpiry - `YYYY-MM-DD` text from the API
 * @param now - The current moment; only the tests pass this
 */
export function isWarrantyExpired(warrantyExpiry: string, now: Date = new Date()): boolean {
  const days = daysUntilDate(warrantyExpiry, now);
  return days !== null && days < 0;
}
