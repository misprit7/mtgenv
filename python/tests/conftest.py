def pytest_configure(config):
    config.addinivalue_line("markers", "slow: longer-running tests (e.g. RL training)")
