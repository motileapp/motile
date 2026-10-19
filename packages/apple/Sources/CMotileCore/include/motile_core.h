// The Rust core's C interface (crates/core/src/ffi.rs). Everything is JSON text.

#ifndef MOTILE_CORE_H
#define MOTILE_CORE_H

#include <stdbool.h>

// Called on a background thread with one event. The text is only valid during the call.
typedef void (*motile_event_callback)(const char *event_json, void *context);

// Starts the core with an `api::Config`. Returns false if it couldn't start.
bool motile_start(const char *config_json, motile_event_callback callback, void *context);

// Sends one command, an `api::Envelope`. The answer arrives as a `reply` event.
void motile_send(const char *command_json);

// Lays out a canvas component (a `canvas::Canvas`) and answers with its `canvas::Scene`, on any
// thread. Free the answer with `motile_free_string`.
char *motile_ui_layout(const char *input_json);

// Builds the page an HtmlView or a YouTube shows (a `web::PageRequest`) and answers with its
// `web::Page`, on any thread. Free the answer with `motile_free_string`.
char *motile_ui_page(const char *input_json);

// Frees a string the core answered with.
void motile_free_string(char *string);

#endif
