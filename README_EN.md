# n8n Desktop

![App Icon](src-tauri/icons/Square150x150Logo.png)

A desktop application for n8n built with Tauri, providing a cross-platform local workflow automation experience. This project aims to simplify n8n installation and usage, offering one-click installation: no manual Node.js environment configuration required, no Docker installation needed.

## 📋 Important Notice

### Copyright and Usage Statement
1. **Project Nature**: This project is a desktop application packaged based on the [n8n](https://github.com/n8n-io/n8n) open-source project, intended for personal learning, research, and testing only.
2. **Non-Commercial Use**: This project must not be used for any commercial purposes, including but not limited to sales, leasing, commercial deployment, etc.
3. **Intellectual Property**: n8n and related trademarks, copyrights belong to their original owners. This project is only a technical packaging and does not own the core intellectual property of n8n.
4. **Infringement Handling**: If this project infringes your legitimate rights, please contact `taoge646@gmail.com`, and we will immediately delete the relevant repository.
5. **Disclaimer**: Any consequences arising from the use of this project shall be borne by the user, and the project maintainers assume no responsibility.

### Security Warning
**Important Security Notice**: This project packages n8n with disabled official restrictions (such as ExecuteCommand nodes).

**Security Risk Warnings**:
1. **Command Injection Risk**: ExecuteCommand nodes allow execution of system commands, malicious workflows may cause data loss, system damage, or security vulnerabilities
2. **Data Security**: Improper use may lead to sensitive data leakage

**Usage Recommendations**:
- Use only in trusted, isolated environments
- Do not use in production environments or systems containing sensitive data
- Carefully review all imported workflows, avoid executing code from unknown sources
- Regularly backup important data

**Disclaimer**:
The developers of this project are not responsible for any data loss caused by using unsafe command injection through ExecuteCommand nodes. Users assume all risks.

### Open Source Licenses
- The code portion of this project uses the MIT License
- n8n core uses the [Sustainable Use License](https://github.com/n8n-io/n8n/blob/master/LICENSE.md)
- Please comply with the respective open-source licenses of each component

## 🚀 Features

- **Cross-Platform Support**: Windows, macOS, Linux full platform support
- **Automatic Dependency Download**: Automatically downloads Node.js runtime and n8n core packages on first run
- **Offline Usage**: Runs locally, protects data privacy

## 📦 Download & Installation

### Latest Version
Visit the [Releases](https://github.com/tangtao646/n8n-desktop/releases) page to download the installation package for your platform:

- **macOS**: `.dmg` file (supports both Intel and Apple Silicon)
- **Windows**: `.exe` installer or `.msi` package
- **Linux**: `.AppImage` or `.deb` package

### System Requirements
- **macOS**: 10.15 (Catalina) or later
- **Windows**: Windows 10 or later (64-bit)
- **Linux**: Mainstream distributions supporting AppImage

### macOS Installation Troubleshooting
If macOS shows "File is damaged" or "Cannot be opened", this is because macOS security mechanisms block unsigned applications. Solution:

1. **Open Terminal**
2. **Execute the following command**:
```bash
sudo xattr -rd com.apple.quarantine /Applications/n8n-desktop.app
```
3. **Enter administrator password** (characters won't be displayed while typing)
4. **Reopen the application**

> **Note**: This command removes the quarantine attribute from the application and should only be used for applications downloaded from trusted sources.

## 🛠️ Development & Building

### Environment Requirements
- Node.js 20+
- Rust 1.70+
- pnpm 8+

### Local Development
```bash
# Clone repository
git clone https://github.com/tangtao646/n8n-desktop.git
cd n8n-desktop

# Install dependencies
pnpm install

# Run in development mode
pnpm tauri dev
```

### Building the Application
```bash
# Build for all platforms
pnpm tauri build

# Build for specific platforms
pnpm tauri build --target universal-apple-darwin  # macOS universal
pnpm tauri build --target x86_64-pc-windows-msi   # Windows
pnpm tauri build --target x86_64-unknown-linux-gnu # Linux
```

### Data Directory
Application data is stored in the user directory:
- **macOS**: `~/Library/Application Support/n8n-desktop/`
- **Windows**: `%APPDATA%\n8n-desktop\`
- **Linux**: `~/.local/share/n8n-desktop/`

Contains:
- `runtime/`: Node.js runtime
- `n8n/`: n8n core files
- `logs/`: Application logs
- `config/`: Configuration files

### Getting Help
- Check the [Issues](https://github.com/tangtao646/n8n-desktop/issues) page
- Submit a new Issue to report problems

## 🤝 Contribution Guidelines

Welcome to submit Issues and Pull Requests!

### Code Standards
- TypeScript: Use ESLint and Prettier
- Rust: Follow Rust official coding standards
- Commit Messages: Use Conventional Commits

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [n8n](https://github.com/n8n-io/n8n) - Powerful workflow automation platform
- [Tauri](https://tauri.app/) - Framework for building small, fast desktop applications
- [React](https://reactjs.org/) - JavaScript library for building user interfaces

## 📞 Contact

If you have questions or suggestions, please contact via:
- **Email**: taoge646@gmail.com
- **GitHub Issues**: [Submit Issue](https://github.com/tangtao646/n8n-desktop/issues)

**Reminder**: This project is for personal learning use only, do not use for commercial purposes. Respect the intellectual property of open-source software and comply with relevant license regulations.