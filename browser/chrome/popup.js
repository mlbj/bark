async function getCurrentTab() {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    return tab;
}

function setStatus(msg) {
    document.getElementById("status").textContent = msg;
}

function nativeMessage(msg) {
    return new Promise((resolve, reject) => {
        chrome.runtime.sendNativeMessage("com.bark.host", msg, (response) => {
            if (chrome.runtime.lastError) {
                reject(new Error(chrome.runtime.lastError.message));
            } else {
                resolve(response);
            }
        });
    });
}

async function showEditor(bibtex, pdf_url) {
    setStatus("Preparing entry…");
    let toml;
    try {
        const res = await nativeMessage({ action: "preview", bibtex, pdf_url });
        if (res.error) throw new Error(res.error);
        toml = res.toml;
    } catch (e) {
        setStatus(`Error: ${e.message}`);
        return;
    }

    document.getElementById("toml-input").value = toml;
    document.getElementById("editor").style.display = "block";
    setStatus("Edit entry and click Import.");

    document.getElementById("import-btn").onclick = async () => {
        const edited = document.getElementById("toml-input").value;
        setStatus("Importing…");
        document.getElementById("import-btn").disabled = true;

        try {
            const res = await nativeMessage({ action: "import", toml: edited });
            if (res.success) {
                setStatus("Imported!");
                document.getElementById("editor").style.display = "none";
            } else {
                setStatus(`Error: ${res.error}`);
                document.getElementById("import-btn").disabled = false;
            }
        } catch (e) {
            setStatus(`Error: ${e.message}`);
            document.getElementById("import-btn").disabled = false;
        }
    };
}

function extractArxivId(url) {
    const m = url.match(/arxiv\.org\/(?:abs|pdf)\/([^?#]+?)(?:\.pdf)?$/);
    return m ? m[1] : null;
}

async function handleArxiv(url) {
    const arxivId = extractArxivId(url);
    if (!arxivId) { setStatus("Could not extract arXiv ID."); return; }

    setStatus("Fetching BibTeX…");
    let bibtex;
    try {
        bibtex = (await (await fetch(`https://arxiv.org/bibtex/${arxivId}`)).text()).trim();
    } catch (e) {
        setStatus(`Fetch failed: ${e.message}`);
        return;
    }

    await showEditor(bibtex, `https://arxiv.org/pdf/${arxivId}`);
}

async function main() {
    const tab = await getCurrentTab();
    if (!tab?.url) { setStatus("No active tab."); return; }

    if (tab.url.includes("arxiv.org")) {
        await handleArxiv(tab.url);
    } else {
        setStatus("Not an arXiv page.");
    }
}

main();
