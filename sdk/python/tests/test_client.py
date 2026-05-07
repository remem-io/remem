"""Tests for the remem Python SDK."""

import pytest


class TestMemoryClient:
    """Unit tests for the Memory client (mocked HTTP)."""

    def test_import(self):
        """Verify the SDK can be imported."""
        from rememhq import Memory, MemoryResult, StoreResponse

        assert Memory is not None
        assert MemoryResult is not None
        assert StoreResponse is not None

    def test_models(self):
        """Verify Pydantic models work."""
        from rememhq.models import MemoryType, ForgetMode

        assert MemoryType.FACT == "fact"
        assert MemoryType.PROCEDURE == "procedure"
        assert ForgetMode.DELETE == "delete"
        assert ForgetMode.ARCHIVE == "archive"

    def test_config_defaults(self):
        """Verify config defaults are sensible."""
        from rememhq.config import RememConfig

        config = RememConfig()
        assert config.base_url == "http://localhost:7474"
        assert config.project == "default"
        assert config.timeout == 30.0

    @pytest.mark.asyncio
    async def test_decay(self, base_url):
        """Verify decay() sends correct request."""
        from rememhq import Memory
        import respx
        import httpx

        async with respx.mock(base_url=base_url) as respx_mock:
            respx_mock.post("/v1/memories/decay").mock(
                return_value=httpx.Response(
                    200, json={"success": True, "archived_count": 5}
                )
            )

            async with Memory(base_url=base_url) as m:
                res = await m.decay(factor=0.5)
                assert res["success"] is True
                assert res["archived_count"] == 5
