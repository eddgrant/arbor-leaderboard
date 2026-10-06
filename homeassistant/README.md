# Home Assistant examples

Ready-made Home Assistant configuration for arbor-leaderboard. Both work in any household as they are: they find your children and your categories from the sensors the service publishes, so you don't need to type in names.

| File | What it gives you |
|---|---|
| [`dashboards/school-lunches.yaml`](dashboards/school-lunches.yaml) | A "School lunches" dashboard: top-ups needed, balances, a leaderboard for every category plus biggest spender, what each child bought on their last school day, and a weekly spend chart |
| [`automations/top-up-reminder.yaml`](automations/top-up-reminder.yaml) | A weekly phone notification listing each child's top-up, which opens Arbor when tapped |

Before you start, make sure arbor-leaderboard is running and its "Arbor *child*" devices appear under **Settings → Devices & services → MQTT**.

## Dashboard

1. **Settings → Dashboards → Add dashboard → New dashboard from scratch**, and name it "School lunches".
2. Open it, then **✏️ Edit → ⋮ → Raw configuration editor**.
3. Paste in [`school-lunches.yaml`](dashboards/school-lunches.yaml) and save.
4. The weekly spend chart is the one card that needs editing: replace its two example entity IDs with your children's **Spend this week** sensors, one line each.

The chart fills in from the day the sensors first appeared, so earlier weeks won't show at first.

## Weekly top-up reminder

1. Install the Home Assistant Companion app on your phone and sign in, so Home Assistant can send it notifications.
2. **Settings → Automations & scenes → Create automation → ⋮ → Edit in YAML**.
3. Paste in [`top-up-reminder.yaml`](automations/top-up-reminder.yaml), then change the two marked lines:
   - the time and day you want the reminder
   - `notify.mobile_app_your_phone`: in **Developer tools → Actions**, type `mobile_app` to find your phone's action
4. Save, and use **⋮ → Run actions** to send yourself a test.

The reminder is skipped in weeks when nobody needs topping up. Tapping it opens the Arbor login page; you can change the link to your school's Arbor address.

## Managing them as code

If your `configuration.yaml` is under version control, the automation can be added to a list included with `automation: !include automations.yaml`, and the dashboard can be a [YAML-mode dashboard](https://www.home-assistant.io/dashboards/dashboards/#adding-yaml-dashboards).
