import SwiftUI

/// Closes the modal a view lies in.
struct CloseModal {
    let action: () -> Void

    func callAsFunction() { action() }
}

extension EnvironmentValues {
    @Entry var closeModal = CloseModal {}
}

/// What `asking` puts to the user: a title, what it means, and the answers besides the one that
/// lets it go. The last answer is the one Return picks.
struct Confirmation {
    let title: String
    var message = ""
    var answers: [Answer] = []
    /// What the button that lets it go says.
    var dismissal = "Cancel"
}

struct Answer {
    let title: String
    var variant: ButtonVariant = .primary
    let action: () -> Void
}

extension View {
    /// Shows the content while `item` is set: our own modal over the window on the Mac, a sheet on
    /// iOS. The content closes it with `closeModal`.
    func modal<Item: Identifiable, Content: View>(item: Binding<Item?>, @ViewBuilder content: @escaping (Item) -> Content) -> some View {
        #if os(macOS)
        modifier(ModalPresenter(item: item, content: content))
        #else
        sheet(item: item) { content($0).sheetSurface() }
        #endif
    }

    func modal<Content: View>(isPresented: Binding<Bool>, @ViewBuilder content: @escaping () -> Content) -> some View {
        #if os(macOS)
        modal(item: Binding { isPresented.wrappedValue ? Presented() : nil } set: { isPresented.wrappedValue = $0 != nil }) { _ in content() }
        #else
        sheet(isPresented: isPresented) { content().sheetSurface() }
        #endif
    }

    /// Asks before something happens, while `item` is set: our own modal on the Mac, the system's
    /// alert on iOS.
    func asking<Item: Identifiable>(_ item: Binding<Item?>, _ confirmation: @escaping (Item) -> Confirmation) -> some View {
        #if os(macOS)
        modal(item: item) { ConfirmationView(confirmation: confirmation($0)) }
        #else
        alert(
            item.wrappedValue.map { confirmation($0).title } ?? "",
            isPresented: Binding { item.wrappedValue != nil } set: { if !$0 { item.wrappedValue = nil } },
            presenting: item.wrappedValue
        ) { shown in
            ForEach(Array(confirmation(shown).answers.reversed().enumerated()), id: \.offset) { _, answer in
                Button(answer.title, role: answer.variant == .danger ? .destructive : nil, action: answer.action)
            }
            Button(confirmation(shown).dismissal, role: .cancel) {}
        } message: { shown in
            Text(confirmation(shown).message)
        }
        #endif
    }
}

private struct Presented: Identifiable {
    var id: Int { 0 }
}
