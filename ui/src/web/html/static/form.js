document.addEventListener("DOMContentLoaded", function (event) {
    // Prevent forms from being submitted several times
    document.querySelectorAll("form").forEach(form => {
        form.addEventListener("submit", (e) => {
            // Prevent if already submitting
            if (form.classList.contains("is-submitting")) {
                e.preventDefault();
            }

            // Add class to hook our visual indicator on
            form.classList.add("is-submitting");
        });
    });
});