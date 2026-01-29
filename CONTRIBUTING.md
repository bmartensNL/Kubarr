# Contributing to Kubarr

Thank you for your interest in contributing to Kubarr! This guide will help you set up your development environment and understand our contribution workflow.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Local Development Setup](#local-development-setup)
- [Building the Project](#building-the-project)
- [Running Tests](#running-tests)
- [Development Workflow](#development-workflow)
- [Pull Request Process](#pull-request-process)
- [Code Style Guidelines](#code-style-guidelines)
- [Remote Development Setup](#remote-development-setup)

## Prerequisites

Before you begin, ensure you have the following installed:

- **Docker** - Required for containerization and Kind cluster
- **Git** - For version control
- **Rust** (1.83 or later) - Backend development
  - Install via [rustup](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** (v18 or later) and **npm** - Frontend development
  - Install via [nvm](https://github.com/nvm-sh/nvm) or download from [nodejs.org](https://nodejs.org/)
- **kubectl** - Kubernetes CLI (auto-installed by setup script if not present)
- **kind** - Kubernetes in Docker (auto-installed by setup script if not present)

### Verify Installation

```bash
docker --version
git --version
rustc --version
node --version
npm --version
```

## Local Development Setup

### 1. Fork and Clone

```bash
# Fork the repository on GitHub, then clone your fork
git clone https://github.com/YOUR_USERNAME/Kubarr.git
cd Kubarr

# Add upstream remote
git remote add upstream https://github.com/ORIGINAL_OWNER/Kubarr.git
```

### 2. Set Up Kind Cluster

Run the local Kubernetes setup script:

```bash
./scripts/local-k8s-setup.sh
```

This script will:
- Create a `bin/` directory for local tools
- Install `kind` and `kubectl` if not present
- Create a Kind cluster named `kubarr` with port mappings
- Configure kubectl context

### 3. Deploy Kubarr

Build and deploy the application:

```bash
./scripts/deploy.sh
```

This will:
- Build backend Docker image (Rust)
- Build frontend Docker image (React)
- Load images into Kind cluster
- Apply Kubernetes manifests
- Restart deployments

### 4. Set Up Port Forwarding

**CRITICAL:** After deployment, you MUST set up port forwarding:

```bash
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 &
```

Verify it's working:

```bash
curl -s http://localhost:8080/api/health
```

Access Kubarr at: **http://localhost:8080**

## Building the Project

### Backend (Rust)

```bash
cd code/backend

# Development build
cargo build

# Fast release build for development
cargo build --profile dev-release

# Full optimized release build
cargo build --release

# Run backend locally (outside Docker)
cargo run
```

**Build Profiles:**
- `dev` - Fast compilation, includes debug symbols
- `dev-release` - Optimized but faster to compile than full release
- `release` - Full optimizations, LTO enabled, stripped binaries

### Frontend (React)

```bash
cd code/frontend

# Install dependencies
npm install

# Start development server
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview

# Lint TypeScript/React code
npm run lint
```

### Docker Builds

```bash
# Build backend image
docker build -f docker/Dockerfile.backend -t kubarr-backend:latest --build-arg PROFILE=dev-release .

# Build frontend image
docker build -f docker/Dockerfile.frontend -t kubarr-frontend:latest .
```

## Running Tests

### Backend Tests

```bash
cd code/backend

# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests with coverage (requires cargo-tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

**Test files are located in:**
- `code/backend/tests/` - Integration tests
- `code/backend/src/` - Unit tests (alongside source code)

### Frontend Tests

```bash
cd code/frontend

# Run Playwright E2E tests
npx playwright test

# Run Playwright in UI mode
npx playwright test --ui

# Run specific test file
npx playwright test tests/example.spec.ts
```

### End-to-End Testing

For full integration testing with the Kind cluster:

1. Deploy Kubarr to Kind cluster (`./scripts/deploy.sh`)
2. Set up port forwarding
3. Run frontend E2E tests against `http://localhost:8080`

## Development Workflow

### Making Changes

1. **Create a feature branch:**
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** following our [Code Style Guidelines](#code-style-guidelines)

3. **Test your changes:**
   ```bash
   # Backend
   cd code/backend && cargo test

   # Frontend
   cd code/frontend && npm run lint && npx playwright test
   ```

4. **Rebuild and deploy:**
   ```bash
   ./scripts/deploy.sh

   # CRITICAL: Restart port forwarding after deploy
   kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 >/dev/null 2>&1 &
   ```

5. **Verify manually** in browser at http://localhost:8080

6. **Commit your changes:**
   ```bash
   git add .
   git commit -m "feat: add your feature description"
   ```

### Commit Message Convention

We follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation changes
- `refactor:` - Code refactoring
- `test:` - Adding or updating tests
- `chore:` - Maintenance tasks
- `perf:` - Performance improvements

**Examples:**
```
feat: add OAuth provider management UI
fix: resolve port forwarding issue after deployment
docs: update API endpoint documentation
refactor: simplify authentication middleware
test: add integration tests for role permissions
```

## Pull Request Process

### Before Submitting

- [ ] Code follows the [Code Style Guidelines](#code-style-guidelines)
- [ ] All tests pass (`cargo test` and `npm run lint`)
- [ ] Documentation is updated if needed
- [ ] Commits follow conventional commit format
- [ ] No debug statements (e.g., `println!`, `console.log`) in production code

### Submitting a PR

1. **Push your branch:**
   ```bash
   git push origin feature/your-feature-name
   ```

2. **Open a Pull Request** on GitHub with:
   - Clear title following conventional commits format
   - Description of changes and motivation
   - Screenshots/videos for UI changes
   - Link to related issues

3. **Respond to review feedback:**
   - Address reviewer comments
   - Push additional commits to the same branch
   - Re-request review when ready

4. **Squash and merge** once approved (maintainers will handle this)

### PR Review Checklist

Reviewers will check:
- Code quality and maintainability
- Test coverage
- Security considerations
- Performance implications
- Documentation completeness

## Code Style Guidelines

### Rust (Backend)

- **Follow Rust conventions**: Use `rustfmt` and `clippy`
  ```bash
  cargo fmt
  cargo clippy -- -D warnings
  ```

- **Error Handling**: Use `Result<T, E>` and `?` operator; avoid `unwrap()` in production code
  ```rust
  // Good
  let value = get_value().map_err(|e| AppError::ConfigError(e.to_string()))?;

  // Avoid
  let value = get_value().unwrap();
  ```

- **Naming Conventions**:
  - `snake_case` for functions, variables, modules
  - `PascalCase` for types, structs, enums
  - `SCREAMING_SNAKE_CASE` for constants

- **Documentation**: Add doc comments for public APIs
  ```rust
  /// Retrieves user by ID from the database.
  ///
  /// # Arguments
  /// * `user_id` - The unique identifier for the user
  ///
  /// # Returns
  /// `Result<User, AppError>` - User model or error
  pub async fn get_user(user_id: Uuid) -> Result<User, AppError> {
      // ...
  }
  ```

### TypeScript/React (Frontend)

- **Follow ESLint rules**: Run `npm run lint`
  ```bash
  npm run lint
  ```

- **Component Structure**:
  - Use functional components with hooks
  - Keep components focused and small
  - Extract reusable logic into custom hooks

  ```tsx
  // Good
  export function UserProfile({ userId }: UserProfileProps) {
    const { data: user, isLoading } = useUser(userId);

    if (isLoading) return <Spinner />;
    return <div>{user?.name}</div>;
  }
  ```

- **Naming Conventions**:
  - `PascalCase` for components and types
  - `camelCase` for functions, variables, hooks
  - `SCREAMING_SNAKE_CASE` for constants

- **Type Safety**: Avoid `any`; prefer explicit types
  ```tsx
  // Good
  interface UserProps {
    userId: string;
    onUpdate: (user: User) => void;
  }

  // Avoid
  interface UserProps {
    userId: any;
    onUpdate: any;
  }
  ```

### General Guidelines

- **No debug statements**: Remove `println!()`, `console.log()`, `dbg!()` before committing
- **Comments**: Write self-documenting code; add comments only when necessary to explain "why"
- **DRY Principle**: Don't repeat yourself - extract common logic
- **KISS Principle**: Keep it simple - avoid over-engineering
- **Security**: Never commit secrets, API keys, or credentials

## Remote Development Setup

For offloading builds to a more powerful server on your local network:

### One-Time Remote Setup

```bash
# Configure remote server, Docker context, and Kind cluster
./scripts/remote-server-setup.sh --host <REMOTE_IP> --user <REMOTE_USER>

# Optionally specify an SSH key
./scripts/remote-server-setup.sh --host <REMOTE_IP> --user <REMOTE_USER> --key ~/.ssh/id_ed25519
```

This script will:
1. Verify SSH key-based authentication to the remote server
2. Check Docker is installed and running on the remote server
3. Create a Docker context named `kubarr-remote` targeting the remote Docker daemon
4. Create a Kind cluster on the remote server
5. Retrieve and merge the remote kubeconfig into `~/.kube/config`

### Remote Build and Deploy

```bash
# Deploy using remote workflow
./scripts/deploy.sh --remote

# CRITICAL: Restart port forwarding after deploy
kubectl --context kind-kubarr port-forward -n kubarr svc/kubarr-backend 8080:8000 >/dev/null 2>&1 &
sleep 2
curl -s http://localhost:8080/api/health  # Verify it works
```

### Switching Between Local and Remote

```bash
# Switch to remote Docker context
docker context use kubarr-remote

# Switch back to local
docker context use default
```

### Remote Troubleshooting

- **`DOCKER_HOST` env var set**: Must be **unset** when using Docker contexts (`unset DOCKER_HOST`)
- **Build context is large**: Optimize `.dockerignore` to minimize transfer size over SSH
- **x509 certificate errors**: Kind cluster must use the remote server's actual IP as `apiServerAddress`
- **Port 6443 unreachable**: Ensure remote server firewall allows connections on port 6443 (Kind API server)
- **SSH connection failures**: Verify SSH key-based auth: `ssh <USER>@<HOST> 'echo OK'`
- **Kind not found on remote**: Kind runs **locally** and targets remote Docker daemon - it does NOT need to be installed on the remote server

## Getting Help

- **Issues**: Search [existing issues](https://github.com/ORIGINAL_OWNER/Kubarr/issues) or create a new one
- **Discussions**: Ask questions in [GitHub Discussions](https://github.com/ORIGINAL_OWNER/Kubarr/discussions)
- **Documentation**: Check [CLAUDE.md](./CLAUDE.md) for detailed development instructions

## License

By contributing to Kubarr, you agree that your contributions will be licensed under the same license as the project.

---

Thank you for contributing to Kubarr! 🚀
