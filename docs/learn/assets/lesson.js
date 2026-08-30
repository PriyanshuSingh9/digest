// Shared JavaScript component for interactive lessons

function initQuiz(widgetId, correctIndex, explanationCorrect, explanationIncorrect) {
  const container = document.getElementById(widgetId);
  if (!container) return;

  const options = container.querySelectorAll('.quiz-option');
  const feedback = container.querySelector('.quiz-feedback');

  options.forEach((opt, idx) => {
    opt.addEventListener('click', () => {
      // Clear previous styles
      options.forEach(o => {
        o.classList.remove('selected-correct', 'selected-incorrect');
      });

      if (idx === correctIndex) {
        opt.classList.add('selected-correct');
        feedback.className = 'quiz-feedback show-correct';
        feedback.innerHTML = `<strong>Correct!</strong> ${explanationCorrect}`;
      } else {
        opt.classList.add('selected-incorrect');
        feedback.className = 'quiz-feedback show-incorrect';
        feedback.innerHTML = `<strong>Not quite.</strong> ${explanationIncorrect}`;
      }
    });
  });
}
