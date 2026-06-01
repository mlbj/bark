async function getCurrentTab() {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    return tab;
}

async function sendToBark(bibtex) {
    return new Promise((resolve) => {
        chrome.runtime.sendNativeMessage(
            "com.bark.host",
            { bibtex },
            (response) => {
                if (chrome.runtime.lastError) {
                    const msg = chrome.runtime.lastError.message;
                    console.error("Native messaging error:", msg);
                    setStatus(`Error: ${msg}`);
                    resolve(false);
                    return;
                }
                resolve(response?.success === true);
            }
        );
    });
}

function extractArxivId(url) {
    const m = url.match(/arxiv\.org\/(?:abs|pdf)\/([^?#]+?)(?:\.pdf)?$/);
    return m ? m[1] : null;
}

async function fetchBibtex(arxivId) {
    const response = await fetch(`https://arxiv.org/bibtex/${arxivId}`);
    return (await response.text()).trim();
}

function setStatus(msg) {
    document.getElementById("status").textContent = msg;
}

async function main() {
    const tab = await getCurrentTab();

    if (!tab?.url) {
        setStatus("No active tab.");
        return;
    }

    if (tab.url.includes("arxiv.org")) {
        await handleArxiv(tab.url);
    } else {
        setStatus("Not an arXiv page.");
    }
}

async function handleArxiv(url) {
    const arxivId = extractArxivId(url);

    if (!arxivId) {
        setStatus("Could not extract arXiv ID.");
        return;
    }

    setStatus("Fetching BibTeX…");

    let bibtex;
    try {
        bibtex = await fetchBibtex(arxivId);
    } catch (e) {
        setStatus(`Fetch failed: ${e.message}`);
        return;
    }

    setStatus("Sending to bark…");
    const ok = await sendToBark(bibtex);
    if (ok) setStatus("Imported!");
}

main();
