import AuthenticationServices
import Tauri
import UIKit

struct AuthenticateArgs: Decodable {
  let url: String
  let callbackScheme: String
}

class WebAuthPlugin: Plugin, ASWebAuthenticationPresentationContextProviding {
  // The session must outlive `authenticate`: the system drops a sheet
  // whose session was deallocated.
  private var session: ASWebAuthenticationSession?

  @objc public func authenticate(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(AuthenticateArgs.self)
    guard let url = URL(string: args.url) else {
      invoke.reject("invalid url")
      return
    }
    DispatchQueue.main.async {
      let session = ASWebAuthenticationSession(
        url: url, callbackURLScheme: args.callbackScheme
      ) { [weak self] callbackURL, error in
        self?.session = nil
        if let error = error as? ASWebAuthenticationSessionError,
          error.code == .canceledLogin
        {
          invoke.reject("cancelled")
        } else if let error = error {
          invoke.reject(error.localizedDescription)
        } else if let callbackURL = callbackURL {
          invoke.resolve(["callbackUrl": callbackURL.absoluteString])
        } else {
          invoke.reject("no callback url")
        }
      }
      session.presentationContextProvider = self
      self.session = session
      if !session.start() {
        self.session = nil
        invoke.reject("could not start the sign-in sheet")
      }
    }
  }

  func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
    let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
    return scenes.flatMap { $0.windows }.first { $0.isKeyWindow } ?? ASPresentationAnchor()
  }
}

@_cdecl("init_plugin_zim_web_auth")
func initPlugin() -> Plugin {
  return WebAuthPlugin()
}
