use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyView, App, Context, DispatchPhase, MouseDownEvent, Pixels, ScrollWheelEvent, Task, Window,
    canvas, div, px,
};

use crate::glass::{lifted, popover_surface, rim};
use crate::motion::{Motion, Motioned as _};
use crate::theme::ActiveTheme as _;

/// How long the pointer rests on a name before its card shows, long enough that a pointer
/// passing over the name on its way elsewhere raises none.
const DELAY: Duration = Duration::from_millis(500);
/// The clear strip between the pointer and the card. It belongs to the card, so the pointer
/// crosses onto the card without leaving it.
const BRIDGE: Pixels = px(6.);
/// How far the card rises into place as it fades in.
const RISE: Pixels = px(4.);

/// Gives an element a hover card: a small raised card about what the element names, shown once
/// the pointer has rested on it for half a second. The card stays while the pointer moves onto
/// it and goes once the pointer has left both, and a scroll, a press or a drag puts it away
/// for the rest of that hover. Only the shared look and the timing live here; `build` makes
/// what the card holds, as a view so it can redraw while its facts arrive.
pub trait HoverCarded: Sized {
    fn hover_card(self, build: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self;
}

impl<E: StatefulInteractiveElement> HoverCarded for E {
    fn hover_card(self, build: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        // gpui is told no delay of its own: the card keeps its own, as the tooltips do, so a
        // cached view does not keep a stale one
        self.tooltip_show_delay(Duration::ZERO)
            .hoverable_tooltip(move |window, cx| {
                let content = build(window, cx);
                cx.new(|cx| HoverCard::new(content, cx)).into()
            })
    }
}

/// The card while one hover lasts: empty through its delay, then the surface around the
/// caller's content.
pub struct HoverCard {
    content: AnyView,
    waiting: Option<Task<()>>,
    /// Set by a scroll, a press or a drag during the hover, which put the card away for good.
    hushed: bool,
}

impl HoverCard {
    fn new(content: AnyView, cx: &mut Context<Self>) -> Self {
        let waiting = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DELAY).await;
            this.update(cx, |this, cx| {
                this.waiting = None;
                cx.notify();
            })
            .ok();
        });
        Self {
            content,
            waiting: Some(waiting),
            hushed: false,
        }
    }
}

impl Render for HoverCard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if cx.has_active_drag() {
            self.hushed = true;
        }
        let me = cx.entity().downgrade();
        // listens while the card waits or shows, which is exactly while this view is drawn
        // A press on the card itself is the card's own, such as its play button.
        let watch = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let scrolled = me.clone();
                window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        hush(&scrolled, cx);
                    }
                });
                let pressed = me;
                window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && !bounds.contains(&event.position) {
                        hush(&pressed, cx);
                    }
                });
            },
        )
        .absolute()
        .inset_0();
        if self.hushed || self.waiting.is_some() {
            return div().child(watch).into_any_element();
        }

        let theme = *cx.theme();
        let card = popover_surface(div(), cx)
            .rounded(rim(theme.radius))
            .border_1()
            .border_color(theme.border)
            .shadow(lifted(cx))
            .overflow_hidden()
            .child(self.content.clone());
        let card = match cx.reduce_motion() {
            true => card.into_any_element(),
            false => card
                .motion("hover-card-in", Motion::Quick, |card, shown| {
                    card.opacity(shown).mt(RISE * (1. - shown))
                })
                .into_any_element(),
        };

        div()
            .pt(BRIDGE)
            .pl(BRIDGE)
            .child(card)
            .child(watch)
            .into_any_element()
    }
}

/// Puts the card away for the rest of the hover.
fn hush(card: &gpui::WeakEntity<HoverCard>, cx: &mut App) {
    card.update(cx, |card, cx| {
        if !card.hushed {
            card.hushed = true;
            card.waiting = None;
            cx.notify();
        }
    })
    .ok();
}
